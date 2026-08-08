import math
import os.path
import traceback

from aperio import PyManagers, gpu_util
from aperio import logger
from aperio import text_rendering
from aperio.audio import AudioManager
from aperio.config_manager import ConfigManager
from aperio.item_structures import *
from aperio.store import StoreManager

# https://stackoverflow.com/questions/42339034/python-module-in-dist-packages-vs-site-packages
# どうやらDebian系Linuxではsite-packagesではなくdist-packagesにインストールされるらしいのでimportされない。
# また、OS管理のPythonを使っているとPYTHONHOMEを設定しているのにもかかわらずそれが適用されないケースが多い。
try:
    import numpy as np
    import numpy.typing as npt
except ImportError as e:
    import traceback as _tb
    logger.error(_tb.format_exc())

    raise ImportError(
        "Failed to import required modules. Make sure numpy are installed."
        "\n--- For Developer ---\nThis error was occured by many complicated reasons. Ensure the below check list and fix them:"
        "\n1. Make sure numpy are installed in the python environment used by Aperio."
        "\n  Running Environment can be checked in below debug info. Generally, required packages should be installed during post running process."
        "\n2. If you are using OS managed python (like apt install python3 on Debian/Ubuntu) to compile and run Aperio, it may cause this error."
        "\n  Please try to install python separately (recommends uv) with `uv python install --reinstall --no-managed-python` and run `./scripts/copy-python.sh --uv`."
        "\n3. For Linux: Make sure that libpython is preloaded as RTLD_GLOBAL correctly. In linux, libpython must be able to be seen globally because of policy of manylinux."
        "\n  Try add the environment LD_PRELOAD to specify the path to libpython3.x.so explicitly."
    ) from e

from .plugin_base.generator_base import (
    AudioGenerateParameters,
    AudioGeneratorReturn,
    VideoGenerateParameters,
)
from .plugin_manager import PluginManager
from .event_manager import EventManager
from .frame_builder import (
    apply_generate_result,
    append_frame_entry,
    collect_additional_item,
    last_leaf_id,
    resolve_additional_entry,
)
from .audio_mixer import mix

# モジュールレベルグローバル — AperioManager.__init__ で設定される
image_generator = gpu_util.PyImageGenerator()
text_renderer = text_rendering.PyTextRenderer(image_generator)
# TODO: PluginManagerに名称を変更し、plugin_managerとして提供
manager: AperioManager
config_manager: ConfigManager
store_manager: StoreManager
audio_manager: AudioManager


class AperioManager(PluginManager, EventManager):
    """
    Aperio のメインマネージャークラス。プラグイン管理・イベント処理・フレーム生成を統括する。
    Rust 側から aperio_plugin.AperioManager として参照される。
    """

    def __init__(self, data_dir: str, managers: PyManagers, plugin_dir_name: str = "plugins"):
        global manager
        global config_manager
        global store_manager
        global audio_manager

        # モジュールレベルグローバルに自身を設定（プラグインから参照されるため）
        manager = self

        # self にも保持（フレーム生成メソッドで直接参照）
        self.generator = image_generator
        self.text_renderer = text_renderer

        # managers から各種マネージャーを取得して設定
        config_manager = managers.config_manager
        store_manager = managers.store_manager
        audio_manager = managers.audio_manager

        shader_dir = os.path.join(os.path.dirname(__file__), "shaders")
        with open(os.path.join(shader_dir, "compose.wgsl"), "r") as f:
            sampler = gpu_util.PySamplerOptions("clamp_to_edge", "linear")
            self.compose_wgsl = gpu_util.PyCompiledWgsl(
                "compose_layer", f.read(), self.generator, sampler
            )
        with open(os.path.join(shader_dir, "fill_black.wgsl"), "r") as f:
            self.fill_black_wgsl = gpu_util.PyCompiledWgsl(
                "fill_black", f.read(), self.generator, None
            )

        # PluginManager.__init__ → プラグインスキャン・インスタンス化
        super().__init__(data_dir, plugin_dir_name)

    def get_fonts_list(self) -> dict[str, list[int]]:
        """
        システムにインストールされているフォントの一覧を返す。

        Returns:
            dict[str, list[int]]: フォントファミリー名 → ウェイト値（100/200/…/900）のリスト
        """
        return self.text_renderer.get_fonts_list()

    def _process_video_item(
        self,
        layer: "ItemStructure.Video",
        frame_number: int,
        width: int,
        height: int,
        structure_id_map: dict[str, tuple[str, int, int]],
    ) -> tuple[gpu_util.PyImageGenerateBuilder, ItemResult, list[AdditionalItem], list[AdditionalItem]] | None:
        """
        1つの Video アイテムの object→effects チェーンを解決し、builder・最終 ItemResult・
        チェーン中に現れた追加アイテム(behind側/ahead側)を返す。

        `_make_frame_builder` のメインループ(実アイテム)と、additionalItem 経由で
        流し込まれる合成アイテムの両方から共通で呼ばれる — 追加アイテムの `item` も
        「他の実アイテムと全く同じコードパス」で処理される。

        `structure_id_map` は「`GenerateStructure.id` → (そのステップの自動採番パイプラインid,
        width, height)」の対応表で、1フレーム分の処理中だけ`_make_frame_builder`が保持する。
        `link_id`が設定された object/effect エントリはプラグインを一切呼ばず、代わりに
        ここから引いた情報でパイプライン上の既存出力をそのまま使い回す(`add_linked`)。
        通常のエントリを処理した後は、その結果をここに登録する。

        Returns:
            None: オブジェクトプラグインが None を返した場合(スキップ対象)。
        """
        behind_items: list[AdditionalItem] = []
        ahead_items: list[AdditionalItem] = []
        # link_idが連続するチェーンでは、実際に`add_linked`をビルダーに反映するのは
        # その連続run内の最後のstepのときだけにする(中間のstepはstructure_id_mapへの
        # エイリアス登録のみ)。Linkedは直前の状態を無視してリンク先のテクスチャに
        # 差し替えるため、連続してadd_linkedすると手前の分が無駄になってしまうため。
        pending_link: tuple[str, int, int] | None = None  # (pipeline_id, width, height)

        object_id = layer.object["id"]
        object_link_id = layer.object.get("link_id")

        if object_link_id is not None:
            target = structure_id_map.get(object_link_id)
            if target is None:
                raise ValueError(f"link_id {object_link_id} was not resolved before use (structure {object_id})")
            structure_id_map[object_id] = target
            pending_link = target
            item_result = ItemResult(target[1], target[2])
            layer_builder = gpu_util.PyImageGenerateBuilder()
        else:
            obj_name = layer.object["name"]
            if obj_name not in self.video_object_plugins:
                raise ValueError(f"Object plugin {obj_name} is not registered")

            obj_plugin = self.video_object_plugins[obj_name]
            params = VideoGenerateParameters(
                frame_number=frame_number,
                layer=layer,
                args=layer.object["parameters"],
                width=width,
                height=height,
                structure_id=object_id,
            )
            layer_frame = obj_plugin.generate(params)
            if layer_frame is None:
                return None
            collect_additional_item(layer_frame.item_result, behind_items, ahead_items)
            item_result = layer_frame.item_result

            layer_builder = gpu_util.PyImageGenerateBuilder()
            if pending_link is not None:
                layer_builder = layer_builder.add_linked(*pending_link)
                pending_link = None
            layer_builder = apply_generate_result(layer_builder, layer_frame)

            pipeline_id_tree = layer_builder.get_id_tree()
            if pipeline_id_tree:  # get_id_treeは常にlist、空ならNoneではなく空listが返る
                structure_id_map[object_id] = (last_leaf_id(pipeline_id_tree), item_result.width, item_result.height)

        for effect in layer.effects:
            effect_id = effect["id"]
            effect_link_id = effect.get("link_id")

            if effect_link_id is not None:
                target = structure_id_map.get(effect_link_id)
                if target is None:
                    raise ValueError(f"link_id {effect_link_id} was not resolved before use (structure {effect_id})")
                structure_id_map[effect_id] = target
                pending_link = target
                item_result = ItemResult(target[1], target[2])
            else:
                if effect["name"] not in self.video_effect_plugins:
                    raise ValueError(f"Effect plugin {effect['name']} is not registered")

                effect_plugin = self.video_effect_plugins[effect["name"]]
                params = VideoGenerateParameters(
                    frame_number=frame_number,
                    layer=layer,
                    args=effect["parameters"],
                    width=item_result.width,
                    height=item_result.height,
                    structure_id=effect_id,
                )
                tmp_layer_frame = effect_plugin.generate(params)
                if tmp_layer_frame is None:
                    continue
                layer_frame = tmp_layer_frame
                collect_additional_item(layer_frame.item_result, behind_items, ahead_items)
                item_result = layer_frame.item_result

                if pending_link is not None:
                    layer_builder = layer_builder.add_linked(*pending_link)
                    pending_link = None
                layer_builder = apply_generate_result(layer_builder, layer_frame)

                pipeline_id_tree = layer_builder.get_id_tree()
                if pipeline_id_tree:
                    structure_id_map[effect_id] = (last_leaf_id(pipeline_id_tree), item_result.width, item_result.height)

        if pending_link is not None:  # チェーン末尾がlinkで終わる場合の materialize
            layer_builder = layer_builder.add_linked(*pending_link)

        return layer_builder, item_result, behind_items, ahead_items

    def _make_frame_builder(
        self,
        frame_number: int,
        frame_structure: list[ItemStructure],
        width: int,
        height: int,
    ) -> tuple[gpu_util.PyImageGenerateBuilder, dict[str, ItemResult]]:
        """
        指定されたフレーム構造に基づいてフレームを生成する内部ヘルパーメソッド。

        このメソッドは公開APIから呼び出されるフレーム生成処理を共通化するために
        切り出されたものであり、クラス外部から直接利用されることは想定していない。

        Args:
            frame_number (int): 生成するフレームの番号
            frame_structure (list[ItemStructure]): フレーム構造のリスト
            width (int): フレームの幅
            height (int): フレームの高さ
        Returns:
            tuple[gpu_util.PyImageGenerateBuilder, dict[str, ItemResult]]: フレーム生成のビルダーオブジェクトとフレーム結果の辞書
        """
        try:
            if not isinstance(frame_structure, list):
                raise TypeError("frame_structure must be a list of ItemStructure")
            if not isinstance(width, int) or not isinstance(height, int):
                raise TypeError("width and height must be integers")
            if width <= 0 or height <= 0:
                raise ValueError("width and height must be positive integers")

            # レイヤーごとにフレームを生成して合成する
            layer_builders = []
            generator_params = []
            frame_results: dict[str, ItemResult] = {}
            structure_id_map: dict[str, tuple[str, int, int]] = {}

            for layer in frame_structure:
                if not isinstance(layer, ItemStructure.Video):
                    logger.warning(f"Layer {layer.id} is not a video layer. Skipping.") # type: ignore
                    continue

                processed = self._process_video_item(layer, frame_number, width, height, structure_id_map)
                if processed is None:
                    continue
                layer_builder, item_result, behind_items, ahead_items = processed
                layer_id = layer.id
                frame_results[layer_id] = item_result

                # additionalItem: チェーン中に現れた追加アイテム(behind/ahead)は、
                # それぞれ実アイテムと全く同じ _process_video_item で解決してから
                # 合成順で背面側/前面側に挿入する。
                behind_entries = [
                    entry
                    for a in behind_items
                    if (entry := resolve_additional_entry(self, a, layer_id, frame_number, width, height, structure_id_map)) is not None
                ]
                ahead_entries = [
                    entry
                    for a in ahead_items
                    if (entry := resolve_additional_entry(self, a, layer_id, frame_number, width, height, structure_id_map)) is not None
                ]

                for behind_entry in behind_entries:
                    append_frame_entry(*behind_entry, layer_builders, generator_params)
                append_frame_entry(layer_builder, layer, item_result, layer_builders, generator_params)
                for ahead_entry in ahead_entries:
                    append_frame_entry(*ahead_entry, layer_builders, generator_params)

            if len(layer_builders) == 0:
                layer_builder = gpu_util.PyImageGenerateBuilder().add_wgsl(
                    self.fill_black_wgsl, None, width, height
                )
                return layer_builder, {}

            builder = (
                gpu_util.PyImageGenerateBuilder()
                .add_parallel_wgsl(layer_builders)
                .add_wgsl(
                    self.compose_wgsl, b"".join(generator_params), width, height
                ) # TODO: render Passを使っての高速化と簡潔化を試みる
            )

        except Exception as e:
            logger.error(traceback.format_exc())
            raise RuntimeError(f"Failed to build frame pipeline: {e}") from e

        return builder, frame_results

    def make_frame_buf(
        self,
        frame_number: int,
        frame_structure: list[ItemStructure],
        width: int,
        height: int,
        buffer_ptr: int,
    ) -> dict[str, ItemResult]:
        """
        指定されたフレーム構造に基づいてフレームを生成し、指定されたバッファに書き込むメソッド。

        Args:
            frame_number (int): 生成するフレームの番号
            frame_structure (list[ItemStructure]): フレーム構造のリスト
            width (int): フレームの幅
            height (int): フレームの高さ
            buffer_ptr (int): 書き込み先バッファのポインタ

        Returns:
            dict[str, ItemResult]: 各レイヤーのフレーム生成結果の辞書
        """
        builder, results = self._make_frame_builder(frame_number, frame_structure, width, height)

        if builder is not None:
            self.generator.generate_buf(builder, buffer_ptr)

        return results

    def make_frame_shared_texture(
        self,
        frame_number: int,
        frame_structure: list[ItemStructure],
        width: int,
        height: int,
        texture_handle: gpu_util.PySharedTextureHandle,
        format: gpu_util.WrappedSharedTextureFormat,
    ) -> dict[str, ItemResult]:
        """
        指定されたフレーム構造に基づいてフレームを生成し、指定された共有テクスチャに書き込むメソッド。

        Args:
            frame_number (int): 生成するフレームの番号
            frame_structure (list[ItemStructure]): フレーム構造のリスト
            width (int): フレームの幅
            height (int): フレームの高さ
            texture_handle (gpu_util.PySharedTextureHandle): 書き込み先の共有テクスチャハンドル
            format (gpu_util.WrappedSharedTextureFormat): 共有テクスチャのフォーマット

        Returns:
            dict[str, ItemResult]: 各レイヤーのフレーム生成結果の辞書
        """
        builder, results = self._make_frame_builder(frame_number, frame_structure, width, height)

        if builder is not None:
            self.generator.generate_shared_texture(builder, texture_handle, format)

        return results

    def _process_audio_item(
        self,
        layer: "ItemStructure.Audio",
        start_time: float,
        sample_rate: int,
        channels: int,
        sample_count: int,
    ) -> tuple[AudioGeneratorReturn, list[AdditionalItem]] | None:
        """
        1つの Audio アイテムの object→effects チェーンを解決し、最終 AudioGeneratorReturn と
        チェーン中に現れた追加アイテムの配列を返す。

        `_make_audio_sample` のメインループ(実アイテム)と、additionalItem 経由で
        流し込まれる合成アイテムの両方から共通で呼ばれる。
        """
        obj_name = layer.object["name"]
        if obj_name not in self.audio_object_plugins:
            raise ValueError(f"Audio object plugin {obj_name} is not registered")

        additional_items: list[AdditionalItem] = []

        obj_plugin = self.audio_object_plugins[obj_name]
        obj_params = AudioGenerateParameters(
            start_time=start_time,
            layer=layer,
            sample_rate=sample_rate,
            channels=channels,
            sample_count=sample_count,
            args=layer.object["parameters"],
        )
        layer_result = obj_plugin.generate(obj_params)
        if layer_result is None:
            return None
        if layer_result.additional_item is not None:
            additional_items.append(layer_result.additional_item)

        # エフェクトを順番に適用（各エフェクトは前段サンプルをinput_samplesで受け取る）
        for effect in layer.effects:
            effect_name = effect["name"]
            if effect_name not in self.audio_effect_plugins:
                raise ValueError(f"Audio effect plugin {effect_name} is not registered")
            effect_plugin = self.audio_effect_plugins[effect_name]
            effect_params = AudioGenerateParameters(
                start_time=start_time,
                layer=layer,
                sample_rate=sample_rate,
                channels=channels,
                sample_count=sample_count,
                args=effect["parameters"],
                input_samples=layer_result.samples,
            )
            tmp_result = effect_plugin.generate(effect_params)
            if tmp_result is None:
                return None
            layer_result = tmp_result
            if layer_result.additional_item is not None:
                additional_items.append(layer_result.additional_item)

        return layer_result, additional_items

    def _make_audio_sample(self, audio_structure: list[ItemStructure], sample_rate: int, channels: int, start_time: float, sample_count: int) -> npt.NDArray[np.float32]:
        """
        指定されたオーディオ構造に基づいてオーディオサンプルを生成するメソッド。

        Args:
            audio_structure (list[ItemStructure]): オーディオ構造のリスト
            sample_rate (int): サンプルレート
            channels (int): チャンネル数
            start_time (float): 生成を開始する時間（秒）
            sample_count (int): 生成するサンプル数

        Returns:
            npt.NDArray[np.float32]: 生成されたオーディオサンプルの2次元配列（チャンネル数 x サンプル数）
        """
        try:
            fps = store_manager.get_state().frame_state.fps

            output = np.zeros((channels, sample_count), dtype=np.float32)
            request_end_time = start_time + sample_count / sample_rate

            for layer in audio_structure:
                if not isinstance(layer, ItemStructure.Audio):
                    logger.warning(f"Layer {layer.id} is not an audio layer. Skipping.") # type: ignore
                    continue

                # フレームを時間に変換
                item_start_time = layer.start / fps
                item_end_time = layer.end / fps

                # リクエスト時間範囲とのオーバーラップ計算
                overlap_start = max(start_time, item_start_time)
                overlap_end = min(request_end_time, item_end_time)

                if overlap_start >= overlap_end:
                    logger.warning(f"Layer {layer.id} has no overlap with requested audio range. Skipping.")
                    continue

                # 出力バッファ内のサンプルオフセットと生成サンプル数
                sample_offset = round((overlap_start - start_time) * sample_rate)
                overlap_sample_count = round((overlap_end - overlap_start) * sample_rate)

                if overlap_sample_count <= 0:
                    logger.warning(f"Layer {layer.id} has no overlapping samples to generate. Skipping.")
                    continue

                processed = self._process_audio_item(layer, overlap_start, sample_rate, channels, overlap_sample_count)
                if processed is None:
                    continue
                layer_result, additional_items = processed
                layer_id = layer.id

                # ボリュームとパンをnumpyで一括適用して加算（浮動小数点丸め誤差による境界超過をクリップ）
                mix(
                    output, channels, sample_count,
                    layer.object["name"], overlap_sample_count, layer_result.samples, layer.volume, layer.pan, sample_offset,
                )

                # additionalItem: 同じ時間窓に加算ミックスする追加のオーディオアイテムがあれば、
                # それぞれ実アイテムと全く同じ _process_audio_item で解決してから同じ窓に加算する
                # (音声は加算合成なので順序に意味が無く、behind は無視してよい。
                for additional in additional_items:
                    if not isinstance(additional.item, ItemStructure.Audio):
                        logger.warning(f"additional_item of layer {layer_id} is not an audio item. Skipping.")
                        continue
                    additional_processed = self._process_audio_item(
                        additional.item, overlap_start, sample_rate, channels, overlap_sample_count
                    )
                    if additional_processed is not None:
                        additional_result, _ = additional_processed
                        mix(
                            output,
                            channels,
                            sample_count,
                            additional.item.object["name"],
                            overlap_sample_count,
                            additional_result.samples,
                            additional.item.volume,
                            additional.item.pan,
                            sample_offset,
                        )

            np.clip(output, -1.0, 1.0, out=output)
            return output

        except Exception as e:
            logger.error(traceback.format_exc())
            raise RuntimeError(f"Failed to generate audio sample: {e}") from e

    def play_audio(self, audio_structure: list[ItemStructure], sample_rate: int, channels: int, start_time: float, duration: float):
        """
        指定されたオーディオ構造に基づいてオーディオを生成し、再生するメソッド。

        Args:
            audio_structure (list[ItemStructure]): オーディオ構造のリスト
            sample_rate (int): サンプルレート
            channels (int): チャンネル数
            start_time (float): 再生を開始する時間（秒）
            duration (float): 再生する期間（秒）
        """
        samples = self._make_audio_sample(audio_structure, sample_rate, channels, start_time, math.floor(sample_rate * duration))  # 指定された期間分のサンプルを生成して再生
        start_sample = round(start_time * sample_rate)
        audio_manager.stack_audio(samples, start_sample)
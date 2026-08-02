import math
import os.path
import struct
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
    GeneratorBuilderReturn,
    GeneratorFuncReturn,
    GeneratorTextureReturn,
    GeneratorWgslReturn,
)
from .plugin_manager import PluginManager
from .event_manager import EventManager

# スピーカーアジマス角 (度数法): 0=正面、負=左、正=右、±180=背面、None=LFE(常にgain=1.0)
# 各エントリはFFmpegのデフォルトチャンネルレイアウト順に準拠
_CHANNEL_AZIMUTHS: dict[int, list[float | None]] = {
    1: [0.0],                                                    # Mono: FC
    2: [-30.0, 30.0],                                            # Stereo: FL FR
    3: [-30.0, 30.0, 0.0],                                       # 3.0: FL FR FC
    4: [-30.0, 30.0, -110.0, 110.0],                            # 4.0: FL FR BL BR
    5: [-30.0, 30.0, 0.0, -110.0, 110.0],                       # 5.0: FL FR FC BL BR
    6: [-30.0, 30.0, 0.0, None, -110.0, 110.0],                 # 5.1: FL FR FC LFE BL BR
    7: [-30.0, 30.0, 0.0, None, -110.0, 110.0, 180.0],         # 6.1: FL FR FC LFE BL BR BC
    8: [-30.0, 30.0, 0.0, None, -110.0, 110.0, -60.0, 60.0],  # 7.1: FL FR FC LFE BL BR SL SR
}


def _apply_generate_result(
    builder: gpu_util.PyImageGenerateBuilder,
    generate_result: "GeneratorWgslReturn | GeneratorFuncReturn | GeneratorTextureReturn | GeneratorBuilderReturn",
) -> gpu_util.PyImageGenerateBuilder:
    """GeneratorWgslReturn/FuncReturn/TextureReturn/BuilderReturn のいずれかを builder に適用する。
    _process_video_item のオブジェクト生成・エフェクトチェーンの両方から使う共通ヘルパー。"""
    item_result = generate_result.item_result
    if isinstance(generate_result, GeneratorWgslReturn):
        return builder.add_wgsl(generate_result.compiled, generate_result.params, item_result.width, item_result.height)
    elif isinstance(generate_result, GeneratorFuncReturn):
        return builder.add_func(generate_result.compiled, generate_result.params, item_result.width, item_result.height)
    elif isinstance(generate_result, GeneratorTextureReturn):
        return builder.add_texture_func(
            generate_result.compiled, generate_result.params, item_result.width, item_result.height
        )
    elif isinstance(generate_result, GeneratorBuilderReturn):
        return builder.add_builder(generate_result.builder)
    return builder


def _compute_pan_gains(pan: float, channels: int) -> npt.NDArray[np.float32]:
    """pan [-1, 1] をスピーカーアジマスに基づくチャンネルごとのゲインに変換する。
    未定義チャンネル数の場合はすべて 1.0 を返す。"""
    azimuths = _CHANNEL_AZIMUTHS.get(channels)
    if azimuths is None:
        return np.ones(channels, dtype=np.float32)
    pan_rad = math.radians(pan * 90.0)
    gains = [
        1.0 if az is None else max(0.0, math.cos(pan_rad - math.radians(az)))
        for az in azimuths
    ]
    return np.array(gains, dtype=np.float32)


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
    ) -> tuple[gpu_util.PyImageGenerateBuilder, ItemResult] | None:
        """
        1つの Video アイテムの object→effects チェーンを解決し、builder と最終 ItemResult を返す。

        `_make_frame_builder` のメインループ(実アイテム)と、additionalObject 経由で
        流し込まれる合成アイテムの両方から共通で呼ばれる — エフェクトが返す
        `ItemResult.additional_object.item` も「他の実アイテムと全く同じコードパス」で
        処理されることがこの関数の存在意義。

        Returns:
            None: オブジェクトプラグインが None を返した場合(スキップ対象)。
        """
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
        )
        layer_frame = obj_plugin.generate(params)
        if layer_frame is None:
            return None

        layer_builder = _apply_generate_result(gpu_util.PyImageGenerateBuilder(), layer_frame)

        for effect in layer.effects:
            if effect["name"] not in self.video_effect_plugins:
                raise ValueError(f"Effect plugin {effect['name']} is not registered")

            effect_plugin = self.video_effect_plugins[effect["name"]]
            params = VideoGenerateParameters(
                frame_number=frame_number,
                layer=layer,
                args=effect["parameters"],
                width=layer_frame.item_result.width,
                height=layer_frame.item_result.height,
            )
            tmp_layer_frame = effect_plugin.generate(params)
            if tmp_layer_frame is None:
                continue
            layer_frame = tmp_layer_frame
            layer_builder = _apply_generate_result(layer_builder, layer_frame)

        return layer_builder, layer_frame.item_result

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

            def _append_entry(item_builder: gpu_util.PyImageGenerateBuilder, item: "ItemStructure.Video", result: ItemResult) -> None:
                eff_x = item.x + (result.x or 0)
                eff_y = item.y + (result.y or 0)
                eff_rotation = item.rotation + (result.rotate or 0.0)
                eff_center_x = result.center_x or 0
                eff_center_y = result.center_y or 0

                # 回転をラジアンに変換してから回転行列を計算
                rotation_rad = math.radians(eff_rotation)
                cos_theta = math.cos(rotation_rad)
                sin_theta = math.sin(rotation_rad)
                alpha = item.alpha / 100  # 0-100 -> 0-1
                rotation_matrix = [cos_theta, sin_theta, -sin_theta, cos_theta]

                fmt = "<iiff"  # x, y, scale, alpha
                fmt += "4f"    # rotation_matrix (2x2 floats)
                fmt += "ii"    # center_x, center_y
                params_bytes = struct.pack(fmt, eff_x, eff_y, item.scale / 100, alpha, *rotation_matrix, eff_center_x, eff_center_y)
                layer_builders.append(item_builder)
                generator_params.append(params_bytes)

            for layer in frame_structure:
                if not isinstance(layer, ItemStructure.Video):
                    logger.warning(f"Layer {layer.id} is not a video layer. Skipping.") # type: ignore
                    continue

                processed = self._process_video_item(layer, frame_number, width, height)
                if processed is None:
                    continue
                layer_builder, item_result = processed
                frame_results[layer.id] = item_result

                # additionalObject: 自分の背面/前面に挿入する追加アイテムがあれば、
                # 実アイテムと全く同じ _process_video_item で解決してから
                # 配列上の隣接位置(=合成順で背面/前面)に挿入する。
                behind_entry = None
                front_entry = None
                additional = item_result.additional_object
                if additional is not None:
                    if not isinstance(additional.item, ItemStructure.Video):
                        logger.warning(f"additional_object of layer {layer.id} is not a video item. Skipping.")
                    else:
                        additional_processed = self._process_video_item(additional.item, frame_number, width, height)
                        if additional_processed is not None:
                            entry = (additional_processed[0], additional.item, additional_processed[1])
                            if additional.behind:
                                behind_entry = entry
                            else:
                                front_entry = entry

                if behind_entry is not None:
                    _append_entry(*behind_entry)
                _append_entry(layer_builder, layer, item_result)
                if front_entry is not None:
                    _append_entry(*front_entry)

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
    ) -> AudioGeneratorReturn | None:
        """
        1つの Audio アイテムの object→effects チェーンを解決し、最終 AudioGeneratorReturn を返す。

        `_make_audio_sample` のメインループ(実アイテム)と、additionalObject 経由で
        流し込まれる合成アイテムの両方から共通で呼ばれる。
        """
        obj_name = layer.object["name"]
        if obj_name not in self.audio_object_plugins:
            raise ValueError(f"Audio object plugin {obj_name} is not registered")

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

        return layer_result

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

            def _mix(obj_name: str, expected_count: int, samples, volume: float, pan: float, sample_offset: int) -> None:
                if samples.ndim != 2 or samples.shape[0] != channels:
                    raise ValueError(
                        f"Audio plugin {obj_name} returned samples with invalid shape {samples.shape}, expected ({channels}, {expected_count})"
                    )
                pan_gains = _compute_pan_gains(pan / 100.0, channels)
                mixed = samples * (volume / 100.0 * pan_gains[:, np.newaxis])
                write_count = min(expected_count, sample_count - sample_offset)
                output[:, sample_offset:sample_offset + write_count] += mixed[:, :write_count]

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

                layer_result = self._process_audio_item(layer, overlap_start, sample_rate, channels, overlap_sample_count)
                if layer_result is None:
                    continue

                # ボリュームとパンをnumpyで一括適用して加算（浮動小数点丸め誤差による境界超過をクリップ）
                _mix(layer.object["name"], overlap_sample_count, layer_result.samples, layer.volume, layer.pan, sample_offset)

                # additionalObject: 同じ時間窓に加算ミックスする追加のオーディオアイテムがあれば、
                # 実アイテムと全く同じ _process_audio_item で解決してから同じ窓に加算する
                # (音声は加算合成なので順序に意味が無く、behind は無視してよい)。
                additional = layer_result.additional_object
                if additional is not None:
                    if not isinstance(additional.item, ItemStructure.Audio):
                        logger.warning(f"additional_object of layer {layer.id} is not an audio item. Skipping.")
                    else:
                        additional_result = self._process_audio_item(
                            additional.item, overlap_start, sample_rate, channels, overlap_sample_count
                        )
                        if additional_result is not None:
                            _mix(
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
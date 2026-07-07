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
            for layer in frame_structure:
                if not isinstance(layer, ItemStructure.Video):
                    logger.warning(f"Layer {layer.id} is not a video layer. Skipping.") # type: ignore
                    continue
                layer_builder = gpu_util.PyImageGenerateBuilder()
                obj_name = layer.object["name"]
                layer_id = layer.id

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
                    continue
                frame_results[layer_id] = layer_frame.item_result
                if isinstance(layer_frame, GeneratorWgslReturn):
                    layer_builder = layer_builder.add_wgsl(
                        layer_frame.compiled,
                        layer_frame.params,
                        layer_frame.item_result.width,
                        layer_frame.item_result.height,
                    )
                elif isinstance(layer_frame, GeneratorFuncReturn):
                    layer_builder = layer_builder.add_func(
                        layer_frame.compiled,
                        layer_frame.params,
                        layer_frame.item_result.width,
                        layer_frame.item_result.height,
                    )
                elif isinstance(layer_frame, GeneratorTextureReturn):
                    layer_builder = layer_builder.add_texture_func(
                        layer_frame.compiled,
                        layer_frame.params,
                        layer_frame.item_result.width,
                        layer_frame.item_result.height,
                    )
                elif isinstance(layer_frame, GeneratorBuilderReturn):
                    layer_builder = layer_builder.add_builder(layer_frame.builder)

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

                    frame_results[layer_id] = layer_frame.item_result
                    if isinstance(layer_frame, GeneratorWgslReturn):
                        layer_builder = layer_builder.add_wgsl(
                            layer_frame.compiled,
                            layer_frame.params,
                            layer_frame.item_result.width,
                            layer_frame.item_result.height,
                        )
                    elif isinstance(layer_frame, GeneratorFuncReturn):
                        layer_builder = layer_builder.add_func(
                            layer_frame.compiled,
                            layer_frame.params,
                            layer_frame.item_result.width,
                            layer_frame.item_result.height,
                        )
                    elif isinstance(layer_frame, GeneratorTextureReturn):
                        layer_builder = layer_builder.add_texture_func(
                            layer_frame.compiled,
                            layer_frame.params,
                            layer_frame.item_result.width,
                            layer_frame.item_result.height,
                        )
                    elif isinstance(layer_frame, GeneratorBuilderReturn):
                        layer_builder = layer_builder.add_builder(layer_frame.builder)

                layer_builders.append(layer_builder)

                # params準備
                result = frame_results[layer_id]
                eff_x = layer.x + (result.x or 0)
                eff_y = layer.y + (result.y or 0)
                eff_rotation = layer.rotation + (result.rotate or 0.0)
                eff_center_x = result.center_x or 0
                eff_center_y = result.center_y or 0

                # 回転をラジアンに変換してから回転行列を計算
                rotation_rad = math.radians(eff_rotation)
                cos_theta = math.cos(rotation_rad)
                sin_theta = math.sin(rotation_rad)
                alpha = layer.alpha / 100  # 0-100 -> 0-1
                rotation_matrix = [cos_theta, sin_theta, -sin_theta, cos_theta]

                fmt = "<iiff"  # x, y, scale, alpha
                fmt += "4f"    # rotation_matrix (2x2 floats)
                fmt += "ii"    # center_x, center_y
                params_bytes = struct.pack(fmt, eff_x, eff_y, layer.scale / 100, alpha, *rotation_matrix, eff_center_x, eff_center_y)
                generator_params.append(params_bytes)

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
                obj_name = layer.object["name"]
                if obj_name not in self.audio_object_plugins:
                    raise ValueError(f"Audio object plugin {obj_name} is not registered")

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

                # オブジェクトプラグインでサンプル生成
                obj_plugin = self.audio_object_plugins[obj_name]
                obj_params = AudioGenerateParameters(
                    start_time=overlap_start,
                    layer=layer,
                    sample_rate=sample_rate,
                    channels=channels,
                    sample_count=overlap_sample_count,
                    args=layer.object["parameters"],
                )
                layer_samples = obj_plugin.generate(obj_params)

                if layer_samples is None:
                    continue

                # エフェクトを順番に適用（各エフェクトは前段サンプルをinput_samplesで受け取る）
                for effect in layer.effects:
                    if layer_samples is None:
                        break
                    effect_name = effect["name"]
                    if effect_name not in self.audio_effect_plugins:
                        raise ValueError(f"Audio effect plugin {effect_name} is not registered")
                    effect_plugin = self.audio_effect_plugins[effect_name]
                    effect_params = AudioGenerateParameters(
                        start_time=overlap_start,
                        layer=layer,
                        sample_rate=sample_rate,
                        channels=channels,
                        sample_count=overlap_sample_count,
                        args=effect["parameters"],
                        input_samples=layer_samples,
                    )
                    layer_samples = effect_plugin.generate(effect_params)

                if layer_samples is None:
                    continue

                if layer_samples.ndim != 2 or layer_samples.shape[0] != channels:
                    raise ValueError(
                        f"Audio plugin {obj_name} returned samples with invalid shape {layer_samples.shape}, expected ({channels}, {overlap_sample_count})"
                    )

                # ボリュームとパンをnumpyで一括適用
                pan_gains = _compute_pan_gains(layer.pan / 100.0, channels)
                layer_samples = layer_samples * (layer.volume / 100.0 * pan_gains[:, np.newaxis])

                # 出力バッファに加算（浮動小数点丸め誤差による境界超過をクリップ）
                write_count = min(overlap_sample_count, sample_count - sample_offset)
                output[:, sample_offset:sample_offset + write_count] += layer_samples[:, :write_count]

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
import math
import os.path
import struct
import traceback

from aperio import gpu_util
from aperio import logger
from aperio import text_rendering
from aperio.frame_structure import *

# https://stackoverflow.com/questions/42339034/python-module-in-dist-packages-vs-site-packages
# どうやらDebian系Linuxではsite-packagesではなくdist-packagesにインストールされるらしいのでimportされない。
# また、OS管理のPythonを使っているとPYTHONHOMEを設定しているのにもかかわらずそれが適用されないケースが多い。
try:
    import numpy as _
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
    GenerateParameters,
    GeneratorFuncReturn,
    GeneratorTextureReturn,
    GeneratorWgslReturn,
)
from .plugin_manager import PluginManager
from .event_manager import EventManager

# モジュールレベルグローバル — AperioManager.__init__ で設定される
image_generator = gpu_util.PyImageGenerator()
text_renderer = text_rendering.PyTextRenderer(image_generator)
manager: AperioManager


class AperioManager(PluginManager, EventManager):
    """
    Aperio のメインマネージャークラス。プラグイン管理・イベント処理・フレーム生成を統括する。
    Rust 側から aperio_plugin.AperioManager として参照される。
    """

    def __init__(self, data_dir: str, plugin_dir_name: str = "plugins"):
        global manager

        # モジュールレベルグローバルに自身を設定（プラグインから参照されるため）
        manager = self

        # self にも保持（フレーム生成メソッドで直接参照）
        self.generator = image_generator
        self.text_renderer = text_renderer

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

    def _make_frame(
        self,
        frame_number: int,
        frame_structure: list[ItemStructure],
        width: int,
        height: int,
        fps: float,
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
            fps (float): フレームレート
        Returns:
            tuple[gpu_util.PyImageGenerateBuilder, dict[str, ItemResult]]: フレーム生成のビルダーオブジェクトとフレーム結果の辞書
        """
        try:
            if not isinstance(frame_structure, list):
                raise TypeError("frame_structure must be a list of ItemStructure")
            if not all(isinstance(layer, dict) for layer in frame_structure):
                raise TypeError("Each layer in frame_structure must be a ItemStructure")
            if not isinstance(width, int) or not isinstance(height, int):
                raise TypeError("width and height must be integers")
            if width <= 0 or height <= 0:
                raise ValueError("width and height must be positive integers")

            # レイヤーごとにフレームを生成して合成する
            layer_builders = []
            generator_params = []
            frame_results: dict[str, ItemResult] = {}
            for layer in frame_structure:
                layer_builder = gpu_util.PyImageGenerateBuilder()
                obj_name = layer["object"]["name"]
                layer_id = layer["id"]

                if obj_name not in self.object_plugins:
                    raise ValueError(f"Object plugin {obj_name} is not registered")

                obj_plugin = self.object_plugins[obj_name]
                params = GenerateParameters(
                    frame_number=frame_number,
                    layer=layer,
                    args=layer["object"]["parameters"],
                    width=width,
                    height=height,
                    fps=fps,
                )
                layer_frame = obj_plugin.generate(params)
                if layer_frame is None:
                    continue
                frame_results[layer_id] = ItemResult(
                    width=layer_frame.output_width,
                    height=layer_frame.output_height,
                )
                if isinstance(layer_frame, GeneratorWgslReturn):
                    layer_builder = layer_builder.add_wgsl(
                        layer_frame.compiled,
                        layer_frame.params,
                        layer_frame.output_width,
                        layer_frame.output_height,
                    )
                elif isinstance(layer_frame, GeneratorFuncReturn):
                    layer_builder = layer_builder.add_func(
                        layer_frame.compiled,
                        layer_frame.params,
                        layer_frame.output_width,
                        layer_frame.output_height,
                    )
                elif isinstance(layer_frame, GeneratorTextureReturn):
                    layer_builder = layer_builder.add_texture_func(
                        layer_frame.compiled,
                        layer_frame.params,
                        layer_frame.output_width,
                        layer_frame.output_height,
                    )

                for effect in layer["effects"]:
                    if effect["name"] not in self.effect_plugins:
                        raise ValueError(f"Effect plugin {effect['name']} is not registered")

                    effect_plugin = self.effect_plugins[effect["name"]]
                    if layer_frame is None:
                        continue
                    params = GenerateParameters(
                        frame_number=frame_number,
                        layer=layer,
                        args=effect["parameters"],
                        width=layer_frame.output_width,
                        height=layer_frame.output_height,
                        fps=fps,
                    )
                    layer_frame = effect_plugin.generate(params)
                    if layer_frame is None:
                        continue
                    frame_results[layer_id] = ItemResult(
                        width=layer_frame.output_width,
                        height=layer_frame.output_height,
                    )
                    if isinstance(layer_frame, GeneratorWgslReturn):
                        layer_builder = layer_builder.add_wgsl(
                            layer_frame.compiled,
                            layer_frame.params,
                            layer_frame.output_width,
                            layer_frame.output_height,
                        )
                    elif isinstance(layer_frame, GeneratorFuncReturn):
                        layer_builder = layer_builder.add_func(
                            layer_frame.compiled,
                            layer_frame.params,
                            layer_frame.output_width,
                            layer_frame.output_height,
                        )
                    elif isinstance(layer_frame, GeneratorTextureReturn):
                        layer_builder = layer_builder.add_texture_func(
                            layer_frame.compiled,
                            layer_frame.params,
                            layer_frame.output_width,
                            layer_frame.output_height,
                        )

                layer_builders.append(layer_builder)

                # params準備
                # 回転をラジアンに変換してから回転行列を計算
                rotation_rad = math.radians(layer["rotation"])
                cos_theta = math.cos(rotation_rad)
                sin_theta = math.sin(rotation_rad)
                alpha = layer["alpha"] / 100  # 0-100 -> 0-1
                rotation_matrix = [cos_theta, sin_theta, -sin_theta, cos_theta]

                fmt = "<iiff"  # x, y, scale, alpha
                fmt += "4f"  # rotation_matrix (2x2 floats)
                params_bytes = struct.pack(fmt, layer["x"], layer["y"], layer["scale"] / 100, alpha, *rotation_matrix)
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
        fps: float,
        buffer_ptr: int,
    ) -> dict[str, ItemResult]:
        """
        指定されたフレーム構造に基づいてフレームを生成し、指定されたバッファに書き込むメソッド。

        Args:
            frame_number (int): 生成するフレームの番号
            frame_structure (list[ItemStructure]): フレーム構造のリスト
            width (int): フレームの幅
            height (int): フレームの高さ
            fps (float): フレームレート
            buffer_ptr (int): 書き込み先バッファのポインタ

        Returns:
            dict[str, ItemResult]: 各レイヤーのフレーム生成結果の辞書
        """
        builder, results = self._make_frame(frame_number, frame_structure, width, height, fps)

        if builder is not None:
            self.generator.generate_buf(builder, buffer_ptr)

        return results

    def make_frame_shared_texture(
        self,
        frame_number: int,
        frame_structure: list[ItemStructure],
        width: int,
        height: int,
        fps: float,
        texture_handle: gpu_util.PySharedTextureHandle,
        format: gpu_util.SharedTextureFormat,
    ) -> dict[str, ItemResult]:
        """
        指定されたフレーム構造に基づいてフレームを生成し、指定された共有テクスチャに書き込むメソッド。

        Args:
            frame_number (int): 生成するフレームの番号
            frame_structure (list[ItemStructure]): フレーム構造のリスト
            width (int): フレームの幅
            height (int): フレームの高さ
            fps (float): フレームレート
            texture_handle (gpu_util.PySharedTextureHandle): 書き込み先の共有テクスチャハンドル
            format (gpu_util.SharedTextureFormat): 共有テクスチャのフォーマット
        
        Returns:
            dict[str, ItemResult]: 各レイヤーのフレーム生成結果の辞書
        """
        builder, results = self._make_frame(frame_number, frame_structure, width, height, fps)

        if builder is not None:
            self.generator.generate_shared_texture(builder, texture_handle, format)

        return results

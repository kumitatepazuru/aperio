import glob
import hashlib
import math
import os.path
import shutil
import struct
import traceback
from typing import Callable
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
    # TODO: 診断情報を出力
    import traceback
    logger.error(traceback.format_exc())

    raise ImportError("Failed to import required modules. Make sure numpy are installed."
                      "\n--- For Developer ---\nThis error was occured by many complicated reasons. Ensure the below check list and fix them:"
                      "\n1. Make sure numpy are installed in the python environment used by Aperio."
                      "\n  Running Environment can be checked in below debug info. Generally, required packages should be installed during post running process."
                      "\n2. If you are using OS managed python (like apt install python3 on Debian/Ubuntu) to compile and run Aperio, it may cause this error."
                      "\n  Please try to install python separately (recommends uv) with `uv python install --reinstall --no-managed-python` and run `./scripts/copy-python.sh --uv`."
                      "\n3. For Linux: Make sure that libpython is preloaded as RTLD_GLOBAL correctly. In linux, libpython must be able to be seen globally because of policy of manylinux."
                      "\n  Try add the environment LD_PRELOAD to specify the path to libpython3.x.so explicitly.") from e

from .plugin_base import MainPluginBase, SubPluginBase
from .plugin_base.generator_base import *


class PluginManager:
    """
    フレーム生成のプラグイン群を管理するクラス。このクラスは、フレーム生成系プラグイン管理の他、フレーム生成を行うためのインターフェースを提供する。
    """

    __plugins: dict[str, type[MainPluginBase]] = {}  # 登録されたプラグインのクラスを保持する辞書
    plugins: dict[str, MainPluginBase] = {}  # 登録されたプラグインのインスタンスを保持する辞書
    object_plugins: dict[str, ObjectGeneratorBase] = {}
    effect_plugins: dict[str, EffectGeneratorBase] = {}

    def __init__(self, data_dir: str, plugin_dir_name="plugins"):
        """
        フレーム生成マネージャーの初期化をする。data_dirはデータディレクトリのパス(通常はget_data_dirによるもの)、plugin_dir_nameはプラグインディレクトリの名前を指定する。
        プラグインディレクトリの構造は以下のようになることを想定している。

        data_dir/
            plugins/
                plugin1/
                    __init__.py
                    (他のプラグインファイル)
                plugin2/
                    __init__.py
                    (他のプラグインファイル)
                ...

        Args:
            data_dir (str): データディレクトリのパス
            plugin_dir_name (str): プラグインディレクトリの名前 (デフォルト: "plugins")
        """

        # openCLが使えるか確認して、有効化
        # if cv2.ocl.haveOpenCL():
        #     cv2.ocl.setUseOpenCL(True)
        #     logger.info(f"OpenCV: OpenCL is available. OpenCL is set to {cv2.ocl.useOpenCL()}")
        # else:
        #     logger.info("OpenCV: OpenCL is not available.")

        self.data_dir = data_dir
        self.plugin_dir_name = plugin_dir_name
        self.generator = gpu_util.PyImageGenerator()
        self.text_renderer = text_rendering.PyTextRenderer(self.generator)

        shader_dir = os.path.join(os.path.dirname(__file__), "shaders")
        with open(os.path.join(shader_dir, "compose.wgsl"), "r") as f:
            sampler = gpu_util.PySamplerOptions("clamp_to_edge", "linear")
            self.compose_wgsl = gpu_util.PyCompiledWgsl("compose_layer", f.read(), self.generator, sampler)
        with open(os.path.join(shader_dir, "fill_black.wgsl"), "r") as f:
            self.fill_black_wgsl = gpu_util.PyCompiledWgsl("fill_black", f.read(), self.generator, None)

        dirs = glob.glob(f"{self.data_dir}/{self.plugin_dir_name}/*")

        # プラグインディレクトリ内の各プラグインをインポートしてデコレータを実行する
        # これにより、self.pluginsにプラグインが自動的に登録される
        for dir in dirs:
            plugin_name = os.path.basename(dir)
            if not os.path.exists(f"{dir}/__init__.py"):
                logger.warning(f"Plugin {plugin_name} does not have an __init__.py file. Skipping.")
                continue

            try:
                __import__(f"{self.plugin_dir_name}.{plugin_name}")
            except Exception as e:
                logger.error(traceback.format_exc())
                logger.error(f"Failed to import plugin {plugin_name}: {e}")

        self.__load_plugins()

    def __load_plugins(self):
        """
        登録されたプラグインのクラスからインスタンスを生成し、self.pluginsに格納するメソッド。
        既に同じ名前のプラグインが存在する場合はスキップする。
        """

        for name, plugin_cls in self.__plugins.items():
            if name in self.plugins:
                logger.info(f"Plugin {name} is already registered. Skipping.")
                continue  # 既に登録されている場合はスキップ

            try:
                plugin_cls.manager = self
                plugin_cls.image_generator = self.generator
                plugin_cls.text_renderer = self.text_renderer

                plugin_instance = plugin_cls()  # PluginManagerのインスタンスを渡す
                self.plugins[name] = plugin_instance
                logger.info(f"Registered plugin: {plugin_instance.name}")
            except Exception as e:
                logger.error(traceback.format_exc())
                logger.error(f"Failed to load plugin {name}: {e}")

            logger.info("Loaded Plugins ---")
            logger.info("\n".join(
                list(map(lambda n: f"{n[0]}(Object)- {n[1].get_display_info()}", self.object_plugins.items()))))
            logger.info("\n".join(
                list(map(lambda n: f"{n[0]}(Effect)- {n[1].get_display_info()}", self.effect_plugins.items()))))

    @classmethod
    def plugin(cls, func: type[MainPluginBase]) -> Callable:
        """
        オブジェクト生成プラグインを登録するためのデコレーター。関数に対して使用し、オブジェクト生成プラグインを登録する。

        Args:
            func (type[MainPluginBase]): オブジェクト生成プラグインのクラス

        Returns:
            Callable: 登録されたオブジェクト生成プラグインのクラス
        """

        if not issubclass(func, MainPluginBase):
            raise TypeError("The decorated class must be a subclass of MainPluginBase")

        cls.__plugins[func.__name__] = func

        def wrapper(*_args, **_kwargs):
            raise RuntimeError("This function is a plugin for Aperio and cannot be called directly")

        return wrapper

    def register_sub_plugin(self, master: MainPluginBase, plugin: SubPluginBase) -> None:
        """
        サブプラグインを登録するメソッド。サブプラグインはObjectGeneratorBaseまたはEffectGeneratorBaseのいずれかを継承している必要がある。

        Args:
            master (MainPluginBase): マスタープラグインのインスタンス
            plugin (SubPluginBase): 登録するサブプラグインのインスタンス
        """

        master_name = master.name
        if not plugin.name.startswith(master_name + "."):
            raise ValueError(f"Sub plugin name {plugin.name} should start with '{master_name}.'. Please rename the plugin or check the master plugin name.")

        if isinstance(plugin, ObjectGeneratorBase):
            self.object_plugins[plugin.name] = plugin
        elif isinstance(plugin, EffectGeneratorBase):
            self.effect_plugins[plugin.name] = plugin
        else:
            raise TypeError("The plugin must be a subclass of ObjectGeneratorBase or EffectGeneratorBase")

    def check_plugin_exists(self, plugin_name: str) -> bool:
        """
        指定された名前のプラグインが存在するかどうかを確認するメソッド。

        Args:
            plugin_name (str): 確認するプラグインの名前

        Returns:
            bool: プラグインが存在する場合はTrue、存在しない場合はFalse
        """
        return plugin_name in self.plugins

    def add_plugin(self, plugin_dir: str) -> bool:
        """
        プラグインを追加するメソッド。
        指定されたディレクトリからプラグインを追加する。既に同じ名前のプラグインが存在する場合は、__init__.pyのハッシュ値を比較して異なる場合のみ更新する。

        Args:
            plugin_dir (str): 追加するプラグインのディレクトリのパス

        Returns:
            bool: プラグインが正常に追加または更新された場合はTrue、それ以外の場合はFalse
        """
        # TODO: URLからのダウンロードや、zipファイルの解凍などもここで行う

        if not os.path.exists(plugin_dir) or not os.path.isdir(plugin_dir):
            logger.error(f"Plugin directory {plugin_dir} does not exist.")
            return False

        plugin_name = os.path.basename(plugin_dir)
        if plugin_name in self.plugins:
            # 既に登録されている場合は__init__.pyのハッシュ値を比較して、異なる場合のみ更新する
            # TODO: バージョン確認で新しければアップデート、古ければ確認みたいにしたい
            logger.info(f"Plugin {plugin_name} is already registered. Trying to update to specified version.")
            if not os.path.exists(f"{plugin_dir}/__init__.py"):
                logger.warning(f"Plugin {plugin_name} does not have an __init__.py file. Skipping.")
                return False

            with open(f"{plugin_dir}/__init__.py", "rb") as f:
                new_hash = hashlib.sha256(f.read()).hexdigest()
                with open(f"{self.data_dir}/{self.plugin_dir_name}/{plugin_name}/__init__.py", "rb") as ef:
                    existing_hash = hashlib.sha256(ef.read()).hexdigest()
                    if new_hash == existing_hash:
                        logger.info(f"Plugin {plugin_name} is completely same. Skipping.")
                        return True

        shutil.copytree(plugin_dir, f"{self.data_dir}/{self.plugin_dir_name}/{plugin_name}", dirs_exist_ok=True)

        # プラグインを再読み込みして登録する
        if not os.path.exists(f"{self.data_dir}/{self.plugin_dir_name}/{plugin_name}/__init__.py"):
            logger.warning(f"Plugin {plugin_name} does not have an __init__.py file after copying. Skipping.")
            return False
        try:
            __import__(f"{self.plugin_dir_name}.{plugin_name}")
        except Exception as e:
            logger.error(f"Failed to import plugin {plugin_name}: {e}")
            return False
        
        logger.info(f"Plugin {plugin_name} has been added/updated.")

        self.__load_plugins()
        return True
    
    def get_fonts_list(self) -> dict[str, list[int]]:
        """
        システムにインストールされているフォントの一覧を返す。

        Returns:
            dict[str, list[int]]: フォントファミリー名 → ウェイト値（100/200/…/900）のリスト
        """
        return self.text_renderer.get_fonts_list()

    def get_plugin_names(self) -> PluginNameInfo:
        """
        登録されているプラグインのnameとdisplay_nameの対応表を取得するメソッド。

        Returns:
            PluginNameInfo: 登録されているプラグインのnameとdisplay_nameの対応表
        """
        result = PluginNameInfo(
            base_plugin={plugin.name: plugin.display_name for plugin in self.plugins.values()},
            object_plugins={name: plugin.display_name for name, plugin in self.object_plugins.items()},
            effect_plugins={name: plugin.display_name for name, plugin in self.effect_plugins.items()}
        )
        
        return result
    
    def request_new_object_generator(self, plugin_name: str, args: dict) -> NewObjectGeneratorReturn:
        """
        指定されたオブジェクトジェネレーターを新規に生成するための情報を取得するメソッド。

        Args:
            plugin_name (str): 生成するオブジェクトジェネレーターの名前
            args (dict): オブジェクトジェネレーターの初期化に必要な任意の引数群

        Returns:
            NewObjectGeneratorReturn: 新しく生成されたオブジェクトジェネレーターの情報
        """
        if plugin_name in self.object_plugins:
            return self.object_plugins[plugin_name].on_new(args)
        else:
            raise ValueError(f"Plugin {plugin_name} is not registered as an object plugin")

    def request_new_effect_generator(self, plugin_name: str) -> NewEffectGeneratorReturn:
        """
        指定されたエフェクトジェネレーターを新規に生成するための情報を取得するメソッド。

        Args:
            plugin_name (str): 生成するエフェクトジェネレーターの名前

        Returns:
            NewEffectGeneratorReturn: 新しく生成されたエフェクトジェネレーターの情報
        """

        if plugin_name in self.effect_plugins:
            return self.effect_plugins[plugin_name].on_new()
        else:
            raise ValueError(f"Plugin {plugin_name} is not registered as an effect plugin")

    def request_parameter_struct(self, plugin_name: str, params: dict) -> list[RequestStructureParameter]:
        """
        指定されたジェネレーターのパラメーター構造を改めてリクエストするメソッド。

        Args:
            plugin_name (str): パラメーター構造をリクエストするジェネレーターの名前
            params (dict): 現在のジェネレーターのパラメータ群。古いRequestStructureParameterを基に構成されている。

        Returns:
            list[RequestStructureParameter]: ジェネレーターのパラメーター構造
        """

        if plugin_name in self.object_plugins:
            return self.object_plugins[plugin_name].on_request_structure(params)
        elif plugin_name in self.effect_plugins:
            return self.effect_plugins[plugin_name].on_request_structure(params)
        else:
            raise ValueError(f"Plugin {plugin_name} is not registered as object or effect plugin")

    def _make_frame(self, frame_number: int, frame_structure: list[ItemStructure], 
                             width: int, height: int, fps: float) -> tuple[gpu_util.PyImageGenerateBuilder, dict[str, ItemResult]]:
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
                    fps=fps
                )
                layer_frame = obj_plugin.generate(params)
                if layer_frame is None:
                    continue
                frame_results[layer_id] = ItemResult(
                    width=layer_frame.output_width,
                    height=layer_frame.output_height
                )
                if isinstance(layer_frame, GeneratorWgslReturn):
                    layer_builder = layer_builder.add_wgsl(layer_frame.compiled, layer_frame.params, 
                                     layer_frame.output_width, layer_frame.output_height)
                elif isinstance(layer_frame, GeneratorFuncReturn):
                    layer_builder = layer_builder.add_func(layer_frame.compiled, layer_frame.params, layer_frame.output_width, layer_frame.output_height)
                elif isinstance(layer_frame, GeneratorTextureReturn):
                    layer_builder = layer_builder.add_texture_func(layer_frame.compiled, layer_frame.params, layer_frame.output_width, layer_frame.output_height)

                # エフェクト適用
                for effect in layer["effects"]:
                    if effect["name"] not in self.effect_plugins:
                        raise ValueError(f"Effect plugin {effect['name']} is not registered")

                    effect_plugin = self.effect_plugins[effect["name"]]
                    if layer_frame is None: # pylanceのための冗長なチェック。実際にはこの時点でlayer_frameがNoneのケースはないはず。
                        continue
                    params = GenerateParameters(
                        frame_number=frame_number,
                        layer=layer,
                        args=effect["parameters"],
                        width=layer_frame.output_width,
                        height=layer_frame.output_height,
                        fps=fps
                    )
                    layer_frame = effect_plugin.generate(params)
                    if layer_frame is None:
                        continue
                    frame_results[layer_id] = ItemResult(
                        width=layer_frame.output_width, 
                        height=layer_frame.output_height
                    )
                    if isinstance(layer_frame, GeneratorWgslReturn):
                        layer_builder = layer_builder.add_wgsl(layer_frame.compiled, layer_frame.params, 
                                         layer_frame.output_width, layer_frame.output_height)
                    elif isinstance(layer_frame, GeneratorFuncReturn):
                        layer_builder = layer_builder.add_func(layer_frame.compiled, layer_frame.params, layer_frame.output_width, layer_frame.output_height)
                    elif isinstance(layer_frame, GeneratorTextureReturn):
                        layer_builder = layer_builder.add_texture_func(layer_frame.compiled, layer_frame.params, layer_frame.output_width, layer_frame.output_height)

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
                # 空のフレーム構造の場合はfill_black.wgslを適用
                layer_builder = gpu_util.PyImageGenerateBuilder().add_wgsl(self.fill_black_wgsl, None, width, height)
                return layer_builder, {}

            # builderを作成
            builder = gpu_util.PyImageGenerateBuilder() \
                .add_parallel_wgsl(layer_builders) \
                .add_wgsl(self.compose_wgsl, b"".join(generator_params), width, height) # TODO: render Passを使っての高速化と簡潔化を試みる

        except Exception as e:
            logger.error(traceback.format_exc())
            raise RuntimeError(f"Failed to build frame pipeline: {e}") from e

        return builder, frame_results
    
    def make_frame_buf(self, frame_number: int, frame_structure: list[ItemStructure], 
                             width: int, height: int, fps: float, buffer_ptr: int) -> dict[str, ItemResult]:
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

    def make_frame_shared_texture(self, frame_number: int, frame_structure: list[ItemStructure], 
                             width: int, height: int, fps: float, texture_handle: gpu_util.PySharedTextureHandle, format: gpu_util.SharedTextureFormat) -> dict[str, ItemResult]:
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

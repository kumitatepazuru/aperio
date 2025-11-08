import glob
import hashlib
import os.path
import shutil
from concurrent.futures.thread import ThreadPoolExecutor
import struct
import time
from typing import Callable
import gpu_util

# https://stackoverflow.com/questions/42339034/python-module-in-dist-packages-vs-site-packages
# どうやらDebian系Linuxではsite-packagesではなくdist-packagesにインストールされるらしいのでimportされない。
# また、OS管理のPythonを使っているとPYTHONHOMEを設定しているのにもかかわらずそれが適用されないケースが多い。
try:
    import cv2
    import numpy as np
except ImportError as e:
    # TODO: 診断情報を出力
    import traceback

    traceback.print_exc()

    raise ImportError("Failed to import required modules. Make sure OpenCV (cv2) and numpy are installed."
                      "\n--- For Developer ---\nThis error was occured by many complicated reasons. Ensure the below check list and fix them:"
                      "\n1. Make sure OpenCV (cv2) and numpy are installed in the python environment used by Aperio."
                      "\n  Running Environment can be checked in below debug info. Generally, required packages should be installed during post running process."
                      "\n2. If you are using OS managed python (like apt install python3 on Debian/Ubuntu) to compile and run Aperio, it may cause this error."
                      "\n  Please try to install python separately (recommends uv) with `uv python install --reinstall --no-managed-python` and run `./scripts/copy-python.sh --uv`."
                      "\n3. For Linux: Make sure that libpython is preloaded as RTLD_GLOBAL correctly. In linux, libpython must be able to be seen globally because of policy of manylinux."
                      "\n  Try add the environment LD_PRELOAD to specify the path to libpython3.x.so explicitly.") from e

from .plugin_base import MainPluginBase, SubPluginBase
from .plugin_base.generator_base import FilterGeneratorBase, GeneratorFuncReturn, GeneratorWgslReturn, ObjectGeneratorBase
from .types.frame_structure import LayerStructure

executor = ThreadPoolExecutor()


class PluginManager:
    """
    フレーム生成のプラグイン群を管理するクラス。このクラスは、フレーム生成系プラグイン管理の他、フレーム生成を行うためのインターフェースを提供する。
    """

    __plugins: dict[str, type[MainPluginBase]] = {}  # 登録されたプラグインのクラスを保持する辞書
    plugins: dict[str, MainPluginBase] = {}  # 登録されたプラグインのインスタンスを保持する辞書
    object_plugins: dict[str, ObjectGeneratorBase] = {}
    filter_plugins: dict[str, FilterGeneratorBase] = {}

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
        if cv2.ocl.haveOpenCL():
            cv2.ocl.setUseOpenCL(True)
            print(f"OpenCV: OpenCL is available. OpenCL is set to {cv2.ocl.useOpenCL()}")
        else:
            print("OpenCV: OpenCL is not available.")

        self.data_dir = data_dir
        self.plugin_dir_name = plugin_dir_name
        self.generator = gpu_util.PyImageGenerator()

        with open(os.path.join(os.path.dirname(__file__), "shaders", "compose.wgsl"), "r") as f:
            sampler = gpu_util.PySamplerOptions("clamp_to_edge", "linear")
            self.compose_wgsl = gpu_util.PyCompiledWgsl("compose_layer", f.read(), self.generator, sampler)

        dirs = glob.glob(f"{self.data_dir}/{self.plugin_dir_name}/*")

        # プラグインディレクトリ内の各プラグインをインポートしてデコレータを実行する
        # これにより、self.pluginsにプラグインが自動的に登録される
        for d in dirs:
            plugin_name = d.split("/")[-1]
            if not os.path.exists(f"{d}/__init__.py"):
                print(f"Plugin {plugin_name} does not have an __init__.py file. Skipping.")
                continue
            __import__(f"{self.plugin_dir_name}.{plugin_name}")

        self.__load_plugins()

    def __load_plugins(self):
        """
        登録されたプラグインのクラスからインスタンスを生成し、self.pluginsに格納するメソッド。
        既に同じ名前のプラグインが存在する場合はスキップする。
        """

        for name, plugin_cls in self.__plugins.items():
            if name in self.plugins:
                print(f"INFO: Plugin {name} is already registered. Skipping.")
                continue  # 既に登録されている場合はスキップ

            try:
                plugin_instance = plugin_cls(self, self.generator)  # PluginManagerのインスタンスを渡す
                self.plugins[name] = plugin_instance
                print(f"Registered plugin: {plugin_instance.name}")
            except Exception as e:
                print(f"Failed to load plugin {name}: {e}")

            print("Loaded Plugins ---")
            print("\n".join(
                list(map(lambda n: f"{n[0]}(Object)- {n[1].get_display_info()}", self.object_plugins.items()))))
            print("\n".join(
                list(map(lambda n: f"{n[0]}(Filter)- {n[1].get_display_info()}", self.filter_plugins.items()))))

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

    def register_sub_plugin(self, plugin: SubPluginBase) -> None:
        """
        サブプラグインを登録するメソッド。サブプラグインはObjectGeneratorBaseまたはFilterGeneratorBaseのいずれかを継承している必要がある。

        Args:
            plugin (SubPluginBase): 登録するサブプラグインのインスタンス
        """

        if isinstance(plugin, ObjectGeneratorBase):
            self.object_plugins[plugin.name] = plugin
        elif isinstance(plugin, FilterGeneratorBase):
            self.filter_plugins[plugin.name] = plugin
        else:
            raise TypeError("The plugin must be a subclass of ObjectGeneratorBase or FilterGeneratorBase")

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
            print(f"Plugin directory {plugin_dir} does not exist.")
            return False

        plugin_name = plugin_dir.split("/")[-1]
        if plugin_name in self.plugins:
            # 既に登録されている場合は__init__.pyのハッシュ値を比較して、異なる場合のみ更新する
            # TODO: バージョン確認で新しければアップデート、古ければ確認みたいにしたい
            print(f"Plugin {plugin_name} is already registered. Trying to update to specified version.")
            if not os.path.exists(f"{plugin_dir}/__init__.py"):
                print(f"Plugin {plugin_name} does not have an __init__.py file. Skipping.")
                return False

            with open(f"{plugin_dir}/__init__.py", "rb") as f:
                new_hash = hashlib.sha256(f.read()).hexdigest()
                with open(f"{self.data_dir}/{self.plugin_dir_name}/{plugin_name}/__init__.py", "rb") as ef:
                    existing_hash = hashlib.sha256(ef.read()).hexdigest()
                    if new_hash == existing_hash:
                        print(f"Plugin {plugin_name} is completely same. Skipping.")
                        return True

        shutil.copytree(plugin_dir, f"{self.data_dir}/{self.plugin_dir_name}/{plugin_name}", dirs_exist_ok=True)

        # プラグインを再読み込みして登録する
        if not os.path.exists(f"{self.data_dir}/{self.plugin_dir_name}/{plugin_name}/__init__.py"):
            print(f"Plugin {plugin_name} does not have an __init__.py file after copying. Skipping.")
            return False
        __import__(f"{self.plugin_dir_name}.{plugin_name}")
        print(f"Plugin {plugin_name} has been added/updated.")

        self.__load_plugins()
        return True

    def make_frame(self, frame_number: int, frame_structure: list[LayerStructure], 
                             width: int, height: int, buffer_ptr: int) -> None:
        """
        指定されたフレーム構造に基づいてフレームを生成するメソッド。

        Args:
            frame_number (int): 生成するフレームの番号
            frame_structure (list[LayerStructure]): フレーム構造のリスト
            width (int): フレームの幅
            height (int): フレームの高さ
            buffer_ptr (int): 書き込み先バッファのポインタ
        """
        try:
            if not isinstance(frame_structure, list):
                raise TypeError("frame_structure must be a list of LayerStructure")
            if not all(isinstance(layer, dict) for layer in frame_structure):
                raise TypeError("Each layer in frame_structure must be a LayerStructure")
            if not isinstance(width, int) or not isinstance(height, int):
                raise TypeError("width and height must be integers")
            if width <= 0 or height <= 0:
                raise ValueError("width and height must be positive integers")
            if len(frame_structure) == 0:
                raise ValueError("frame_structure must contain at least one layer")

            # レイヤーごとにフレームを生成して合成する
            layer_builders = []
            params = []
            for layer in frame_structure:
                layer_builder = gpu_util.PyImageGenerateBuilder()
                obj_name = layer["obj"]["name"]

                if obj_name not in self.object_plugins:
                    raise ValueError(f"Object plugin {obj_name} is not registered")

                obj_plugin = self.object_plugins[obj_name]
                layer_frame = obj_plugin.generate(frame_number, layer["obj"]["parameters"], width, height)
                if isinstance(layer_frame, GeneratorWgslReturn):
                    layer_builder = layer_builder.add_wgsl(layer_frame.compiled, layer_frame.params, 
                                     layer_frame.output_width, layer_frame.output_height)
                elif isinstance(layer_frame, GeneratorFuncReturn):
                    layer_builder = layer_builder.add_func(layer_frame.compiled, layer_frame.params,
                                      layer_frame.output_width, layer_frame.output_height)

                # エフェクト適用
                for effect in layer["effects"]:
                    if effect["name"] not in self.filter_plugins:
                        raise ValueError(f"Filter plugin {effect['name']} is not registered")

                    filter_plugin = self.filter_plugins[effect["name"]]
                    layer_frame = filter_plugin.generate(frame_number, effect["parameters"], width, height)
                    if isinstance(layer_frame, GeneratorWgslReturn):
                        layer_builder = layer_builder.add_wgsl(layer_frame.compiled, layer_frame.params, 
                                         layer_frame.output_width, layer_frame.output_height)
                    elif isinstance(layer_frame, GeneratorFuncReturn):
                        layer_builder = layer_builder.add_func(layer_frame.compiled, layer_frame.params,
                                          layer_frame.output_width, layer_frame.output_height)

                layer_builders.append(layer_builder)

                # params準備
                fmt = "<iif"  # x, y, scale
                params_bytes = struct.pack(fmt, layer["x"], layer["y"], layer["scale"])
                params.append(params_bytes)

            # GPU処理実行
            builder = gpu_util.PyImageGenerateBuilder() \
                .add_parallel_wgsl(layer_builders) \
                .add_wgsl(self.compose_wgsl, b"".join(params), width, height)

            # 直接バッファに書き込み
            self.generator.generate(builder, buffer_ptr)

        except Exception as e:
            import traceback
            traceback.print_exc()
            raise RuntimeError(f"Failed to make frame: {e}")


    def make_frames(self, start_frame_number: int, amount: int, *args, **kwargs):
        """
        指定された数だけフレームをmultithreadingで生成するメソッド。make_frameと同じ引数を受け取り、amountで指定された数だけフレームを生成してリストで返す。

        Args:
            start_frame_number (int): 生成を開始するフレームの番号
            amount (int): 生成するフレームの数
            *args: make_frameに渡す引数
            **kwargs: make_frameに渡すキーワード引数

        Returns:
            list[np.ndarray]: 生成されたフレームのリスト
        """
        try:
            if not isinstance(amount, int) or amount <= 0:
                raise ValueError("amount must be a positive integer")

            frames = []
            futures = [executor.submit(self.make_frame, start_frame_number + i, *args, **kwargs)
                       for i in range(amount)]
            for future in futures:
                frames.append(future.result())


            return frames
        except Exception as e:
            import traceback
            traceback.print_exc()
            raise RuntimeError(f"Failed to make frames: {e}")

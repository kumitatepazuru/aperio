import glob
import hashlib
import os.path
import shutil
import time
import traceback
from typing import Callable, ClassVar

from aperio import logger
from aperio.item_structures import PluginNameInfo

from .plugin_base import MainPluginBase, SubPluginBase
from .plugin_base.generator_base import *
from .plugin_load_progress import PluginLoadProgressWindow

# TODO: PluginLoaderに改名、ファイル名も改名
class PluginManager:
    """
    プラグインの登録・読み込み・追加を管理するクラス。
    プラグインディレクトリのスキャン、クラスのインスタンス化、サブプラグインの登録を担う。
    """

    __plugins: ClassVar[dict[str, type[MainPluginBase]]] = {}
    plugins: dict[str, MainPluginBase]
    video_object_plugins: dict[str, VideoObjectGeneratorBase]
    video_effect_plugins: dict[str, VideoEffectGeneratorBase]
    audio_object_plugins: dict[str, AudioObjectGeneratorBase]
    audio_effect_plugins: dict[str, AudioEffectGeneratorBase]

    def __init__(self, data_dir: str, plugin_dir_name: str = "plugins"):
        self.data_dir = data_dir
        self.plugin_dir_name = plugin_dir_name
        self.plugins = {}
        self.video_object_plugins = {}
        self.video_effect_plugins = {}
        self.audio_object_plugins = {}
        self.audio_effect_plugins = {}

        self._progress: PluginLoadProgressWindow | None = PluginLoadProgressWindow()
        self._bar3_master: MainPluginBase | None = None

        dirs = glob.glob(f"{self.data_dir}/{self.plugin_dir_name}/*")

        self._progress.set_bar1(50, "プラグインを登録中")
        self._progress.start_bar2(len(dirs), "")

        for dir in dirs:
            plugin_name = os.path.basename(dir)
            if not os.path.exists(f"{dir}/__init__.py"):
                logger.warning(
                    f"Plugin {plugin_name} does not have an __init__.py file. Skipping."
                )
                self._progress.step_bar2(plugin_name)
                continue

            try:
                __import__(f"{self.plugin_dir_name}.{plugin_name}")
            except Exception as e:
                logger.error(traceback.format_exc())
                logger.error(f"Failed to import plugin {plugin_name}: {e}")

            self._progress.step_bar2(plugin_name)

        self.__load_plugins()

        self._progress.close()
        self._progress = None

    def __load_plugins(self):
        """
        登録されたプラグインのクラスからインスタンスを生成し self.plugins に格納する。
        既に同じ名前のプラグインが存在する場合はスキップする。
        self.generator / self.text_renderer は AperioManager.__init__ で設定済みであること。
        """
        logger.info("Beginning to load plugins...")
        t = time.perf_counter()

        if self._progress:
            self._progress.set_bar1(100, "サブプラグインを登録中")
            self._progress.start_bar2(len(self.__plugins), "")

        for name, plugin_cls in self.__plugins.items():
            if self._progress:
                self._progress.step_bar2(name)
            if name in self.plugins:
                logger.warning(f"Plugin {name} is already registered. Skipping.")
                continue

            self._bar3_master = None
            if self._progress:
                self._progress.hide_bar3()

            try:
                plugin_instance = plugin_cls()
                self.plugins[name] = plugin_instance
                logger.info(f"Registered plugin: {plugin_instance.name}")
            except Exception as e:
                logger.error(traceback.format_exc())
                logger.error(f"Failed to load plugin {name}: {e}")
        logger.info(f"Total plugins loaded: {len(self.plugins)}, including {len(self.video_object_plugins)} video object plugins, {len(self.video_effect_plugins)} video effect plugins, {len(self.audio_object_plugins)} audio object plugins, and {len(self.audio_effect_plugins)} audio effect plugins.")
        logger.info(f"it takes {time.perf_counter() - t:.2f} seconds.")

        logger.info("Loaded Plugins ---")
        logger.info(
            "\n".join(
                [
                    f"{n}(Video Object)- {p.get_display_info()}"
                    for n, p in self.video_object_plugins.items()
                ]
            )
        )
        logger.info(
            "\n".join(
                [
                    f"{n}(Video Effect)- {p.get_display_info()}"
                    for n, p in self.video_effect_plugins.items()
                ]
            )
        )
        logger.info(
            "\n".join(
                [
                    f"{n}(Audio Object)- {p.get_display_info()}"
                    for n, p in self.audio_object_plugins.items()
                ]
            )
        )
        logger.info(
            "\n".join(
                [
                    f"{n}(Audio Effect)- {p.get_display_info()}"
                    for n, p in self.audio_effect_plugins.items()
                ]
            )
        )

    @classmethod
    def plugin(cls, func: type[MainPluginBase]) -> Callable:
        """
        MainPluginBase サブクラスをプラグインとして登録するデコレーター。

        Args:
            func: 登録する MainPluginBase のサブクラス
        """
        if not issubclass(func, MainPluginBase):
            raise TypeError("The decorated class must be a subclass of MainPluginBase")

        if func.__name__ in cls.__plugins:
            logger.warning(
                f"A plugin with the name '{func.__name__}' is already registered."
            )
            index = 1
            for name in cls.__plugins.keys():
                if name.startswith(func.__name__):
                    index += 1

            cls.__plugins[f"{func.__name__}_{index}"] = func
        else:
            cls.__plugins[func.__name__] = func

        def wrapper(*_args, **_kwargs):
            raise RuntimeError(
                "This function is a plugin for Aperio and cannot be called directly"
            )

        return wrapper

    def register_sub_plugin(self, master: MainPluginBase, plugin: SubPluginBase) -> None:
        """
        ObjectGeneratorBase または EffectGeneratorBase のサブプラグインを登録する。

        Args:
            master: マスタープラグインのインスタンス
            plugin: 登録するサブプラグインのインスタンス
        """
        master_name = master.name
        if not plugin.name.startswith(master_name + "."):
            raise ValueError(
                f"Sub plugin name '{plugin.name}' should start with '{master_name}.'. "
                "Please rename the plugin or check the master plugin name."
            )

        if isinstance(plugin, VideoObjectGeneratorBase):
            self.video_object_plugins[plugin.name] = plugin
        elif isinstance(plugin, VideoEffectGeneratorBase):
            self.video_effect_plugins[plugin.name] = plugin
        elif isinstance(plugin, AudioObjectGeneratorBase):
            self.audio_object_plugins[plugin.name] = plugin
        elif isinstance(plugin, AudioEffectGeneratorBase):
            self.audio_effect_plugins[plugin.name] = plugin
        else:
            raise TypeError(
                "The plugin must be a subclass of ObjectGeneratorBase or EffectGeneratorBase"
            )

        if self._progress and master.num_sub_plugins >= 1:
            if self._bar3_master is not master:
                self._progress.start_bar3(master.num_sub_plugins, plugin.display_name)
                self._bar3_master = master
            self._progress.step_bar3(plugin.display_name)

    def check_plugin_exists(self, plugin_name: str) -> bool:
        """
        指定された名前のプラグインが存在するかどうかを確認する。

        Args:
            plugin_name: 確認するプラグインの名前

        Returns:
            プラグインが存在する場合は True
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

        self._progress = PluginLoadProgressWindow()
        self._bar3_master = None
        try:
            if not os.path.exists(plugin_dir) or not os.path.isdir(plugin_dir):
                logger.error(f"Plugin directory {plugin_dir} does not exist.")
                return False

            plugin_name = os.path.basename(plugin_dir)
            self._progress.set_bar1(50, "プラグインを登録中")
            self._progress.start_bar2(1, plugin_name)

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

            self._progress.step_bar2(plugin_name)
            logger.info(f"Plugin {plugin_name} has been added/updated.")
            self.__load_plugins()
            return True
        finally:
            self._progress.close()
            self._progress = None

    def get_plugin_names(self) -> PluginNameInfo:
        """
        登録されているプラグインの name と display_name の対応表を返す。

        Returns:
            PluginNameInfo: プラグイン名と表示名の辞書
        """
        return PluginNameInfo(
            base_plugin={plugin.name: plugin.display_name for plugin in self.plugins.values()},
            video_object_plugins={name: plugin.display_name for name, plugin in self.video_object_plugins.items()},
            video_effect_plugins={name: plugin.display_name for name, plugin in self.video_effect_plugins.items()},
            audio_object_plugins={name: plugin.display_name for name, plugin in self.audio_object_plugins.items()},
            audio_effect_plugins={name: plugin.display_name for name, plugin in self.audio_effect_plugins.items()},
        )

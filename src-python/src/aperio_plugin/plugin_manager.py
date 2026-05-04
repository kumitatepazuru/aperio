import glob
import hashlib
import os.path
import shutil
import traceback
from typing import Callable, ClassVar

from aperio import logger
from aperio.frame_structure import PluginNameInfo

from .plugin_base import MainPluginBase, SubPluginBase
from .plugin_base.generator_base import EffectGeneratorBase, ObjectGeneratorBase


class PluginManager:
    """
    プラグインの登録・読み込み・追加を管理するクラス。
    プラグインディレクトリのスキャン、クラスのインスタンス化、サブプラグインの登録を担う。
    """

    __plugins: ClassVar[dict[str, type[MainPluginBase]]] = {}
    plugins: dict[str, MainPluginBase]
    object_plugins: dict[str, ObjectGeneratorBase]
    effect_plugins: dict[str, EffectGeneratorBase]

    def __init__(self, data_dir: str, plugin_dir_name: str = "plugins"):
        self.data_dir = data_dir
        self.plugin_dir_name = plugin_dir_name
        self.plugins = {}
        self.object_plugins = {}
        self.effect_plugins = {}

        dirs = glob.glob(f"{self.data_dir}/{self.plugin_dir_name}/*")

        for dir in dirs:
            plugin_name = os.path.basename(dir)
            if not os.path.exists(f"{dir}/__init__.py"):
                logger.warning(
                    f"Plugin {plugin_name} does not have an __init__.py file. Skipping."
                )
                continue

            try:
                __import__(f"{self.plugin_dir_name}.{plugin_name}")
            except Exception as e:
                logger.error(traceback.format_exc())
                logger.error(f"Failed to import plugin {plugin_name}: {e}")

        self.__load_plugins()

    def __load_plugins(self):
        """
        登録されたプラグインのクラスからインスタンスを生成し self.plugins に格納する。
        既に同じ名前のプラグインが存在する場合はスキップする。
        self.generator / self.text_renderer は AperioManager.__init__ で設定済みであること。
        """
        for name, plugin_cls in self.__plugins.items():
            if name in self.plugins:
                logger.info(f"Plugin {name} is already registered. Skipping.")
                continue

            try:
                plugin_instance = plugin_cls()
                self.plugins[name] = plugin_instance
                logger.info(f"Registered plugin: {plugin_instance.name}")
            except Exception as e:
                logger.error(traceback.format_exc())
                logger.error(f"Failed to load plugin {name}: {e}")

        logger.info("Loaded Plugins ---")
        logger.info(
            "\n".join(
                [
                    f"{n}(Object)- {p.get_display_info()}"
                    for n, p in self.object_plugins.items()
                ]
            )
        )
        logger.info(
            "\n".join(
                [
                    f"{n}(Effect)- {p.get_display_info()}"
                    for n, p in self.effect_plugins.items()
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

        if isinstance(plugin, ObjectGeneratorBase):
            self.object_plugins[plugin.name] = plugin
        elif isinstance(plugin, EffectGeneratorBase):
            self.effect_plugins[plugin.name] = plugin
        else:
            raise TypeError(
                "The plugin must be a subclass of ObjectGeneratorBase or EffectGeneratorBase"
            )

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

    def get_plugin_names(self) -> PluginNameInfo:
        """
        登録されているプラグインの name と display_name の対応表を返す。

        Returns:
            PluginNameInfo: プラグイン名と表示名の辞書
        """
        return PluginNameInfo(
            base_plugin={plugin.name: plugin.display_name for plugin in self.plugins.values()},
            object_plugins={name: plugin.display_name for name, plugin in self.object_plugins.items()},
            effect_plugins={name: plugin.display_name for name, plugin in self.effect_plugins.items()},
        )

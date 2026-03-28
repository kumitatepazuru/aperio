from .. import PluginManager
from .generator_base import FilterGeneratorBase as FilterGeneratorBase, ObjectGeneratorBase as ObjectGeneratorBase
from _typeshed import Incomplete
from dataclasses import dataclass
from gpu_util import PyImageGenerator as PyImageGenerator

@dataclass
class PluginNameInfo:
    base_plugin: dict[str, str]
    object_plugins: dict[str, str]
    filter_plugins: dict[str, str]

class PluginBase:
    """
    プラグインの基底クラス。 サブクラスでオーバーライドして使用することを想定している。
    """
    name: str
    display_name: str
    description: str
    def __init__(self) -> None:
        """
        プラグインの初期化を行う。必要に応じてサブクラスでオーバーライドする。
        """
    def get_display_info(self) -> str:
        """
        プラグインの情報を表示用フォーマットで返却するメソッド。必要に応じてサブクラスでオーバーライドする。
        """

class SubPluginBase(PluginBase):
    """
    処理系の基底クラス。 サブクラスでオーバーライドして使用することを想定している。
    """

class MainPluginBase(PluginBase):
    """
    プラグイン全体の基底クラス。 サブクラスでオーバーライドして使用することを想定している。
    plugin_filesには、GeneratorBaseを継承したクラスを指定する。システムは、このリストに基づいてジェネレーターを認識する。
    """
    version: str
    author: str
    manager: Incomplete
    def __init__(self, manager: PluginManager, generator: PyImageGenerator) -> None:
        """
        プラグインの初期化を行う。必要に応じてサブクラスでオーバーライドする。
        """
    def get_display_info(self) -> str:
        """
        プラグインの情報を表示用フォーマットで返却するメソッド。必要に応じてサブクラスでオーバーライドする。
        """

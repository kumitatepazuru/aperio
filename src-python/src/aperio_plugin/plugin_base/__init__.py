from __future__ import annotations

from typing import TYPE_CHECKING

from gpu_util import PyImageGenerator

if TYPE_CHECKING:
    from .. import PluginManager


class PluginBase:
    """
    プラグインの基底クラス。 サブクラスでオーバーライドして使用することを想定している。
    """

    def __init__(self) -> None:
        """
        プラグインの初期化を行う。必要に応じてサブクラスでオーバーライドする。
        """
        self.name = "BasePlugin"
        self.display_name = "Base Plugin"
        self.description = "This is a base plugin class."

    def get_display_info(self) -> str:
        """
        プラグインの情報を表示用フォーマットで返却するメソッド。必要に応じてサブクラスでオーバーライドする。
        """

        return f"{self.display_name}\n\t{self.description}"


class SubPluginBase(PluginBase):
    """
    処理系の基底クラス。 サブクラスでオーバーライドして使用することを想定している。
    """

    pass


class MainPluginBase(PluginBase):
    """
    プラグイン全体の基底クラス。 サブクラスでオーバーライドして使用することを想定している。
    plugin_filesには、GeneratorBaseを継承したクラスを指定する。システムは、このリストに基づいてジェネレーターを認識する。
    """

    def __init__(self, manager: PluginManager, generator: PyImageGenerator) -> None:
        """
        プラグインの初期化を行う。必要に応じてサブクラスでオーバーライドする。
        """

        super().__init__()
        self.version = "0.1.0"
        self.author = "Your Name"
        self.manager = manager

    def get_display_info(self) -> str:
        """
        プラグインの情報を表示用フォーマットで返却するメソッド。必要に応じてサブクラスでオーバーライドする。
        """

        return f"{self.display_name} v{self.version} by {self.author}\n\t{self.description}"

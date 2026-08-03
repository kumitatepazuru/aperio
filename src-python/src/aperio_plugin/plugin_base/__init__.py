from __future__ import annotations

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
        self.is_hidden = False
        """True の場合、UI のプラグイン一覧(オブジェクト追加メニュー等)から除外される。
        内部専用のサブプラグイン(additionalItem の解決用など)に使う。"""

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

    def __init__(self) -> None:
        """
        プラグインの初期化を行う。必要に応じてサブクラスでオーバーライドする。
        """

        super().__init__()
        self.version = "0.1.0"
        self.author = "Your Name"
        self.num_sub_plugins = -1

    def get_display_info(self) -> str:
        """
        プラグインの情報を表示用フォーマットで返却するメソッド。必要に応じてサブクラスでオーバーライドする。
        """

        return f"{self.display_name} v{self.version} by {self.author}\n\t{self.description}"

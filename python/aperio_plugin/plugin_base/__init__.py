from __future__ import annotations

from typing import TYPE_CHECKING, ClassVar

if TYPE_CHECKING:
    from .generator_base import GeneratorKind


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
        内部専用のジェネレーター(additionalItem の解決用など)に使う。"""

    def get_display_info(self) -> str:
        """
        プラグインの情報を表示用フォーマットで返却するメソッド。必要に応じてサブクラスでオーバーライドする。
        """

        return f"{self.display_name}\n\t{self.description}"


class GeneratorBase(PluginBase):
    """
    ジェネレーター(処理系)の基底クラス。 サブクラスでオーバーライドして使用することを想定している。
    メインプラグインの初期化中に ``aperio_plugin.plugin_manager.add_generator()`` で登録する。
    """

    kind: ClassVar[GeneratorKind]
    """ジェネレーターの種別。VideoObjectGeneratorBase などの種別ごとの基底クラスで設定される。"""


class MainPluginBase(PluginBase):
    """
    プラグイン全体の基底クラス。 サブクラスでオーバーライドして使用することを想定している。
    パッケージの ``[project.entry-points."aperio.plugins"]`` にこのクラスのサブクラスを指定すると、
    システムがインスタンス化する。entry point 名は ``name`` と一致させること。
    """

    def __init__(self) -> None:
        """
        プラグインの初期化を行う。必要に応じてサブクラスでオーバーライドする。
        """

        super().__init__()
        self.version = "0.1.0"
        self.author = "Your Name"

    def get_display_info(self) -> str:
        """
        プラグインの情報を表示用フォーマットで返却するメソッド。必要に応じてサブクラスでオーバーライドする。
        """

        return f"{self.display_name} v{self.version} by {self.author}\n\t{self.description}"

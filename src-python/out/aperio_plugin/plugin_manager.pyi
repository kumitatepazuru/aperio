from .plugin_base.generator_base import *
from .plugin_base import MainPluginBase as MainPluginBase, SubPluginBase as SubPluginBase
from _typeshed import Incomplete
from aperio.item_structures import PluginNameInfo
from typing import Callable

class PluginManager:
    """
    プラグインの登録・読み込み・追加を管理するクラス。
    プラグインディレクトリのスキャン、クラスのインスタンス化、サブプラグインの登録を担う。
    """
    plugins: dict[str, MainPluginBase]
    video_object_plugins: dict[str, VideoObjectGeneratorBase]
    video_effect_plugins: dict[str, VideoEffectGeneratorBase]
    audio_object_plugins: dict[str, AudioObjectGeneratorBase]
    audio_effect_plugins: dict[str, AudioEffectGeneratorBase]
    data_dir: Incomplete
    plugin_dir_name: Incomplete
    def __init__(self, data_dir: str, plugin_dir_name: str = 'plugins') -> None: ...
    @classmethod
    def plugin(cls, func: type[MainPluginBase]) -> Callable:
        """
        MainPluginBase サブクラスをプラグインとして登録するデコレーター。

        Args:
            func: 登録する MainPluginBase のサブクラス
        """
    def register_sub_plugin(self, master: MainPluginBase, plugin: SubPluginBase) -> None:
        """
        ObjectGeneratorBase または EffectGeneratorBase のサブプラグインを登録する。

        Args:
            master: マスタープラグインのインスタンス
            plugin: 登録するサブプラグインのインスタンス
        """
    def check_plugin_exists(self, plugin_name: str) -> bool:
        """
        指定された名前のプラグインが存在するかどうかを確認する。

        Args:
            plugin_name: 確認するプラグインの名前

        Returns:
            プラグインが存在する場合は True
        """
    def add_plugin(self, plugin_dir: str) -> bool:
        """
        プラグインを追加するメソッド。
        指定されたディレクトリからプラグインを追加する。既に同じ名前のプラグインが存在する場合は、__init__.pyのハッシュ値を比較して異なる場合のみ更新する。

        Args:
            plugin_dir (str): 追加するプラグインのディレクトリのパス

        Returns:
            bool: プラグインが正常に追加または更新された場合はTrue、それ以外の場合はFalse
        """
    def get_plugin_names(self) -> PluginNameInfo:
        """
        登録されているプラグインの name と display_name の対応表を返す。

        Returns:
            PluginNameInfo: プラグイン名と表示名の辞書
        """

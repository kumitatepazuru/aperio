from .plugin_base import MainPluginBase, SubPluginBase
from .plugin_base.generator_base import FilterGeneratorBase, ObjectGeneratorBase
from .types.frame_structure import LayerStructure as LayerStructure
from _typeshed import Incomplete
from typing import Callable

executor: Incomplete

class PluginManager:
    """
    フレーム生成のプラグイン群を管理するクラス。このクラスは、フレーム生成系プラグイン管理の他、フレーム生成を行うためのインターフェースを提供する。
    """
    plugins: dict[str, MainPluginBase]
    object_plugins: dict[str, ObjectGeneratorBase]
    filter_plugins: dict[str, FilterGeneratorBase]
    data_dir: Incomplete
    plugin_dir_name: Incomplete
    generator: Incomplete
    compose_wgsl: Incomplete
    def __init__(self, data_dir: str, plugin_dir_name: str = 'plugins') -> None:
        '''
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
        '''
    @classmethod
    def plugin(cls, func: type[MainPluginBase]) -> Callable:
        """
        オブジェクト生成プラグインを登録するためのデコレーター。関数に対して使用し、オブジェクト生成プラグインを登録する。

        Args:
            func (type[MainPluginBase]): オブジェクト生成プラグインのクラス

        Returns:
            Callable: 登録されたオブジェクト生成プラグインのクラス
        """
    def register_sub_plugin(self, plugin: SubPluginBase) -> None:
        """
        サブプラグインを登録するメソッド。サブプラグインはObjectGeneratorBaseまたはFilterGeneratorBaseのいずれかを継承している必要がある。

        Args:
            plugin (SubPluginBase): 登録するサブプラグインのインスタンス
        """
    def check_plugin_exists(self, plugin_name: str) -> bool:
        """
        指定された名前のプラグインが存在するかどうかを確認するメソッド。

        Args:
            plugin_name (str): 確認するプラグインの名前

        Returns:
            bool: プラグインが存在する場合はTrue、存在しない場合はFalse
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
    def make_frame(self, frame_number: int, frame_structure: list[LayerStructure], width: int, height: int, buffer_ptr: int) -> None:
        """
        指定されたフレーム構造に基づいてフレームを生成するメソッド。

        Args:
            frame_number (int): 生成するフレームの番号
            frame_structure (list[LayerStructure]): フレーム構造のリスト
            width (int): フレームの幅
            height (int): フレームの高さ
            buffer_ptr (int): 書き込み先バッファのポインタ
        """
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

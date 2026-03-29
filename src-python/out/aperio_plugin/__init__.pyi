from .plugin_base.generator_base import *
import gpu_util
from .plugin_base import MainPluginBase, PluginNameInfo, SubPluginBase
from .types.frame_structure import LayerStructure as LayerStructure, RequestStructureParameter as RequestStructureParameter
from _typeshed import Incomplete
from typing import Callable

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
    def register_sub_plugin(self, master: MainPluginBase, plugin: SubPluginBase) -> None:
        """
        サブプラグインを登録するメソッド。サブプラグインはObjectGeneratorBaseまたはFilterGeneratorBaseのいずれかを継承している必要がある。

        Args:
            master (MainPluginBase): マスタープラグインのインスタンス
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
    def get_plugin_names(self) -> PluginNameInfo:
        """
        登録されているプラグインのnameとdisplay_nameの対応表を取得するメソッド。

        Returns:
            PluginNameInfo: 登録されているプラグインのnameとdisplay_nameの対応表
        """
    def request_new_object_generator(self, plugin_name: str, args: dict) -> NewObjectGeneratorReturn:
        """
        指定されたオブジェクトジェネレーターを新規に生成するための情報を取得するメソッド。

        Args:
            plugin_name (str): 生成するオブジェクトジェネレーターの名前
            args (dict): オブジェクトジェネレーターの初期化に必要な任意の引数群

        Returns:
            NewObjectGeneratorReturn: 新しく生成されたオブジェクトジェネレーターの情報
        """
    def request_new_filter_generator(self, plugin_name: str) -> NewFilterGeneratorReturn:
        """
        指定されたフィルタージェネレーターを新規に生成するための情報を取得するメソッド。

        Args:
            plugin_name (str): 生成するフィルタージェネレーターの名前

        Returns:
            NewFilterGeneratorReturn: 新しく生成されたフィルタージェネレーターの情報
        """
    def request_parameter_struct(self, plugin_name: str, params: dict) -> list[RequestStructureParameter]:
        """
        指定されたジェネレーターのパラメーター構造を改めてリクエストするメソッド。

        Args:
            plugin_name (str): パラメーター構造をリクエストするジェネレーターの名前
            params (dict): 現在のジェネレーターのパラメータ群。古いRequestStructureParameterを基に構成されている。

        Returns:
            list[RequestStructureParameter]: ジェネレーターのパラメーター構造
        """
    def make_frame_buf(self, frame_number: int, frame_structure: list[LayerStructure], width: int, height: int, buffer_ptr: int) -> None:
        """
        指定されたフレーム構造に基づいてフレームを生成し、指定されたバッファに書き込むメソッド。

        Args:
            frame_number (int): 生成するフレームの番号
            frame_structure (list[LayerStructure]): フレーム構造のリスト
            width (int): フレームの幅
            height (int): フレームの高さ
            buffer_ptr (int): 書き込み先バッファのポインタ
        """
    def make_frame_shared_texture(self, frame_number: int, frame_structure: list[LayerStructure], width: int, height: int, texture_handle: gpu_util.PySharedTextureHandle, format: gpu_util.SharedTextureFormat) -> None:
        """
        指定されたフレーム構造に基づいてフレームを生成し、指定された共有テクスチャに書き込むメソッド。

        Args:
            frame_number (int): 生成するフレームの番号
            frame_structure (list[LayerStructure]): フレーム構造のリスト
            width (int): フレームの幅
            height (int): フレームの高さ
            texture_handle (gpu_util.PySharedTextureHandle): 書き込み先の共有テクスチャハンドル
            format (gpu_util.SharedTextureFormat): 共有テクスチャのフォーマット
        """

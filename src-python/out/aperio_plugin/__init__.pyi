from aperio.frame_structure import *
from .plugin_base.generator_base import *
from .plugin_base import MainPluginBase, SubPluginBase
from _typeshed import Incomplete
from aperio import gpu_util
from typing import Callable

class PluginManager:
    """
    フレーム生成のプラグイン群を管理するクラス。このクラスは、フレーム生成系プラグイン管理の他、フレーム生成を行うためのインターフェースを提供する。
    """
    plugins: dict[str, MainPluginBase]
    object_plugins: dict[str, ObjectGeneratorBase]
    effect_plugins: dict[str, EffectGeneratorBase]
    data_dir: Incomplete
    plugin_dir_name: Incomplete
    generator: Incomplete
    text_renderer: Incomplete
    compose_wgsl: Incomplete
    fill_black_wgsl: Incomplete
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
        サブプラグインを登録するメソッド。サブプラグインはObjectGeneratorBaseまたはEffectGeneratorBaseのいずれかを継承している必要がある。

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
    def get_fonts_list(self) -> dict[str, list[int]]:
        """
        システムにインストールされているフォントの一覧を返す。

        Returns:
            dict[str, list[int]]: フォントファミリー名 → ウェイト値（100/200/…/900）のリスト
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
    def request_new_effect_generator(self, plugin_name: str) -> NewEffectGeneratorReturn:
        """
        指定されたエフェクトジェネレーターを新規に生成するための情報を取得するメソッド。

        Args:
            plugin_name (str): 生成するエフェクトジェネレーターの名前

        Returns:
            NewEffectGeneratorReturn: 新しく生成されたエフェクトジェネレーターの情報
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
    def make_frame_buf(self, frame_number: int, frame_structure: list[ItemStructure], width: int, height: int, fps: float, buffer_ptr: int) -> dict[str, ItemResult]:
        """
        指定されたフレーム構造に基づいてフレームを生成し、指定されたバッファに書き込むメソッド。

        Args:
            frame_number (int): 生成するフレームの番号
            frame_structure (list[ItemStructure]): フレーム構造のリスト
            width (int): フレームの幅
            height (int): フレームの高さ
            fps (float): フレームレート
            buffer_ptr (int): 書き込み先バッファのポインタ

        Returns:
            dict[str, ItemResult]: 各レイヤーのフレーム生成結果の辞書
        """
    def make_frame_shared_texture(self, frame_number: int, frame_structure: list[ItemStructure], width: int, height: int, fps: float, texture_handle: gpu_util.PySharedTextureHandle, format: gpu_util.SharedTextureFormat) -> dict[str, ItemResult]:
        """
        指定されたフレーム構造に基づいてフレームを生成し、指定された共有テクスチャに書き込むメソッド。

        Args:
            frame_number (int): 生成するフレームの番号
            frame_structure (list[ItemStructure]): フレーム構造のリスト
            width (int): フレームの幅
            height (int): フレームの高さ
            fps (float): フレームレート
            texture_handle (gpu_util.PySharedTextureHandle): 書き込み先の共有テクスチャハンドル
            format (gpu_util.SharedTextureFormat): 共有テクスチャのフォーマット
        
        Returns:
            dict[str, ItemResult]: 各レイヤーのフレーム生成結果の辞書
        """

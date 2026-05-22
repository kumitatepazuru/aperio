from aperio.frame_structure import *
from .event_manager import EventManager
from .plugin_manager import PluginManager
from _typeshed import Incomplete
from aperio import gpu_util

image_generator: Incomplete
text_renderer: Incomplete
manager: AperioManager

class AperioManager(PluginManager, EventManager):
    """
    Aperio のメインマネージャークラス。プラグイン管理・イベント処理・フレーム生成を統括する。
    Rust 側から aperio_plugin.AperioManager として参照される。
    """
    generator: Incomplete
    text_renderer: Incomplete
    compose_wgsl: Incomplete
    fill_black_wgsl: Incomplete
    def __init__(self, data_dir: str, plugin_dir_name: str = 'plugins') -> None: ...
    def get_fonts_list(self) -> dict[str, list[int]]:
        """
        システムにインストールされているフォントの一覧を返す。

        Returns:
            dict[str, list[int]]: フォントファミリー名 → ウェイト値（100/200/…/900）のリスト
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

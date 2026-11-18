"""AviUtl 互換の ``obj.*`` 変数の定義と、ItemResult との変換に使う計算。"""

from collections.abc import Iterable, Mapping

from aperio.item_structures import ItemResult, ItemStructure
from lua_runtime import LuaRuntime

WRITABLE_VARIABLES: dict[str, type[float]] = {
    "obj.ox": float,
    "obj.oy": float,
    "obj.oz": float,
    "obj.rx": float,
    "obj.ry": float,
    "obj.rz": float,
    "obj.cx": float,
    "obj.cy": float,
    "obj.cz": float,
    "obj.zoom": float,
    "obj.alpha": float,
    "obj.aspect": float,
}
"""writable変数。スクリプト実行後にItemResultに反映する"""

READONLY_VARIABLES: dict[str, type[float] | type[int]] = {
    "obj.x": float,
    "obj.y": float,
    "obj.z": float,
    "obj.w": float,
    "obj.h": float,
    "obj.screen_w": int,
    "obj.screen_h": int,
    "obj.framerate": float,
    "obj.frame": int,
    "obj.time": float,
    "obj.totalframe": int,
    "obj.totaltime": float,
    "obj.layer": int,
}
"""readonly変数"""


def register(runtime: LuaRuntime) -> None:
    """``obj.*`` 変数をすべてランタイムに登録する。"""
    for name, value_type in (WRITABLE_VARIABLES | READONLY_VARIABLES).items():
        runtime.add_variable(name, value_type, value_type(0))


def zoom_aspect(sx: float, sy: float) -> tuple[float, float]:
    """X/Y の拡大率から、AviUtl の拡大率とアスペクト比(プラス=横縮小/マイナス=縦縮小)を求める。"""
    zoom = max(sx, sy)
    if zoom == 0:
        return 0.0, 0.0
    if sx < sy:
        return zoom, 1.0 - sx / zoom
    return zoom, sy / zoom - 1.0


def scale_from(zoom: float, aspect: float) -> tuple[float, float]:
    """zoom_aspectの逆変換。アスペクト比は -1.0~1.0 に丸める。"""
    aspect = max(-1.0, min(1.0, aspect))
    if aspect >= 0:
        return zoom * (1.0 - aspect), zoom
    return zoom, zoom * (1.0 + aspect)


def find_root_id(parent_id: str, item_results: Mapping[str, ItemResult]) -> str | None:
    """
    追加アイテムの生成元を、親が無くなるまで遡ったアイテムの id を返す。
    どちらにも見つからない、または循環している場合は None。
    """
    current = parent_id
    visited: set[str] = set()
    while current not in visited:
        visited.add(current)
        owner = next(
            (key for key, result in item_results.items() if any(a.item.id == current for a in result.additional_items)),
            None,
        )
        if owner is None:
            return current if current in item_results else None
        current = owner
    return None


def find_timeline_item(item_id: str, timeline_items: Iterable[ItemStructure]) -> ItemStructure.Video | None:
    """タイムライン上のVideo Itemを id で探す。"""
    for item in timeline_items:
        if isinstance(item, ItemStructure.Video) and item.id == item_id:
            return item
    return None

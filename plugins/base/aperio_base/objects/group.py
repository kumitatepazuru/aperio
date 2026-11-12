import math
import uuid

from aperio.item_structures import (
    GenerateStructure,
    GeneratorEvent,
    GeneratorInformation,
    ItemResult,
    ItemStructure,
    RequestStructureParameter,
)
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import VideoGenerateParameters, VideoObjectGeneratorBase


class GroupObject(VideoObjectGeneratorBase):
    def __init__(self):
        super().__init__()

        self.name = "base_object.group_object"
        self.display_name = "グループ"
        self.description = "グループオブジェクト。複数のレイヤーをまとめて扱うことができる。"

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return GeneratorInformation(
            display_name=self.display_name,
            duration_frames=300,
            max_frame=None,
            min_frame=None,
            structure=[
                # 0 = 一番下まで
                RequestStructureParameter.Int("layers", "対象レイヤー数", 0, min=0),
                RequestStructureParameter.Bool("affected_by_parent", "上位グループ制御の影響を受ける", False),
            ],
        )

    @event(type=GeneratorEvent.BeforeMakeFrame)
    def on_before_make_frame(self, frame_structure: list[ItemStructure]) -> list[ItemStructure] | None:
        parent = self._build_parent_table(frame_structure)
        if not parent:
            return None

        result: list[ItemStructure] = []
        for item in frame_structure:
            if not isinstance(item, ItemStructure.Video) or self._is_group(item) or item.layer not in parent:
                result.append(item)
                continue
            result.append(_apply_chain(item, _parent_chain(item, parent)))
        return result

    @event(type=GeneratorEvent.ItemMove)
    @event(type=GeneratorEvent.ItemScale)
    @event(type=GeneratorEvent.ItemRotate)
    def on_item_edit(
        self, old_items: list[ItemStructure], new_items: list[ItemStructure]
    ) -> list[ItemStructure] | None:
        """
        UI は画面上の値のまま編集してくるので、支配するグループの変形を戻してメンバーのパラメーター上の値にする。
        回転(加算)と拡大率(乗算)はグループと合成しても差分が変わらないので、書き換えるのは x/y の差分だけ。
        """
        parent = self._build_parent_table(old_items)
        if not parent:
            return None
        olds = {item.id: item for item in old_items if isinstance(item, ItemStructure.Video)}

        result: list[ItemStructure] = []
        for new in new_items:
            old = olds.get(new.id) if isinstance(new, ItemStructure.Video) else None
            if not isinstance(new, ItemStructure.Video) or old is None or old.layer not in parent:
                result.append(new)
                continue
            # _apply_chain / on_after_make_frame と同じく、画面上の位置 = 拡大率 * 回転(x, y) + 平行移動
            scale, rotation, _, _, _ = _compose_chain(_parent_chain(old, parent))
            if scale == 0:
                result.append(new)
                continue
            dx, dy = _rotate((new.x - old.x) / scale, (new.y - old.y) / scale, -rotation)
            result.append(_with_position(new, old.x + dx, old.y + dy))
        return result

    @event(type=GeneratorEvent.AfterMakeFrame)
    def on_after_make_frame(
        self, frame_structure: list[ItemStructure], results: dict[str, ItemResult]
    ) -> dict[str, ItemResult] | None:
        """グループの ItemResult を、支配下のメンバー全員を囲む大きさに書き換える(UI の枠に使われる)。"""
        groups = [item for item in frame_structure if isinstance(item, ItemStructure.Video) and self._is_group(item)]
        if not groups:
            return None
        parent = self._build_parent_table(frame_structure)

        # グループ id → 支配下のメンバーの枠の角(フレーム座標)。入れ子のグループには内側のメンバーも含める
        corners: dict[str, list[tuple[float, float]]] = {}
        for item in frame_structure:
            if not isinstance(item, ItemStructure.Video) or self._is_group(item) or item.layer not in parent:
                continue
            result = results.get(item.id)
            if result is None:
                continue
            points = _box_corners(result)
            owner = parent[item.layer]
            while True:
                corners.setdefault(owner.id, []).extend(points)
                if owner.layer not in parent:
                    break
                owner = parent[owner.layer]

        new_results = dict(results)
        for group in groups:
            points = corners.get(group.id)
            if not points or group.id not in results:
                continue
            # グループ自身は BeforeMakeFrame で書き換えないので、上位グループの変形をここで合成して絶対値にする
            scale, rotation, x, y, alpha = _compose_chain([group, *_parent_chain(group, parent)])
            if scale == 0:
                continue
            group_result = results[group.id].copy()
            group_result.pos = (x, y, 0.0)
            group_result.scale = (scale, scale)
            group_result.rotate = (0.0, 0.0, rotation)
            group_result.alpha = alpha

            local = [_to_local(p, group_result) for p in points]
            xs = [lx for lx, _ in local]
            ys = [ly for _, ly in local]
            # center は「原点が枠の中心からどれだけずれるか」(拡大前のpx)なので、枠の中心の符号を逆にしたもの
            group_result = group_result.with_size(_ceil_size(max(xs) - min(xs)), _ceil_size(max(ys) - min(ys)))
            group_result.center_x = -(max(xs) + min(xs)) / 2
            group_result.center_y = -(max(ys) + min(ys)) / 2
            new_results[group.id] = group_result
        return new_results

    def generate(self, params: VideoGenerateParameters) -> ItemResult:
        # グループ自身は何も描画しない。枠の大きさはメンバーの生成後に AfterMakeFrame で決まるので、ここでは仮の値を返す
        return params.item_result.with_size(1, 1)

    def _is_group(self, item: ItemStructure) -> bool:
        return (
            isinstance(item, ItemStructure.Video)
            and item.object["name"] == self.name
            and item.object.get("link_id") is None
        )

    def _build_parent_table(self, frame_structure: list[ItemStructure]) -> dict[int, ItemStructure.Video]:
        """
        レイヤー → そのレイヤーを支配するグループ(範囲内にある一番内側のグループ) の表を作る。
        """
        parent: dict[int, ItemStructure.Video] = {}
        # (グループ, 範囲の排他的な終端)。末尾ほど内側
        active: list[tuple[ItemStructure.Video, float]] = []
        videos = [item for item in frame_structure if isinstance(item, ItemStructure.Video)]
        for item in sorted(videos, key=lambda i: i.layer):
            active = [(g, end) for g, end in active if item.layer < end]
            # 同じレイヤーのグループ(終端境界で隣接するアイテム等)は支配者にしない
            owner = next((g for g, _ in reversed(active) if g.layer < item.layer), None)
            is_group = self._is_group(item)
            if owner is not None and (not is_group or item.object["parameters"].get("affected_by_parent", False)):
                parent[item.layer] = owner
            if is_group:
                active.append((item, _range_end(item)))
        return parent


def _parent_chain(item: ItemStructure.Video, parent: dict[int, ItemStructure.Video]) -> list[ItemStructure.Video]:
    """item を支配するグループの鎖(内側 → 外側)。支配するグループが無ければ空。"""
    chain: list[ItemStructure.Video] = []
    layer = item.layer
    while layer in parent:
        chain.append(parent[layer])
        layer = chain[-1].layer
    return chain


def _range_end(group: ItemStructure.Video) -> float:
    """グループの対象範囲の排他的な終端レイヤー。自分の1つ下から n 段、n = 0 なら一番下まで。"""
    n = int(group.object["parameters"].get("layers", 0))
    return math.inf if n == 0 else group.layer + n + 1


def _rotate(x: float, y: float, degrees: float) -> tuple[float, float]:
    """frame_builder.layer_params の Z軸回転と同じ向き(画面 y 下向きで正 = 時計回り)に回す。"""
    rad = math.radians(degrees)
    c, s = math.cos(rad), math.sin(rad)
    return x * c - y * s, x * s + y * c


def _ceil_size(value: float) -> int:
    """枠の大きさを整数pxに切り上げる。回転の逆変換で出る浮動小数点の誤差(60.00000000001 等)で1px増えないよう丸めてから切り上げる。"""
    return max(1, math.ceil(round(value, 4)))


def _box_corners(result: ItemResult) -> list[tuple[float, float]]:
    """アイテムの枠の4つの角をフレーム座標で返す。UI の Overlay.tsx が枠を描く計算と同じ。"""
    box = result.offscreen_source or result
    scale_x, scale_y = box.scale
    pivot_x, pivot_y = box.pos[0], box.pos[1]
    center_x = box.center_x * scale_x
    center_y = box.center_y * scale_y
    half_w = box.width * scale_x / 2
    half_h = box.height * scale_y / 2

    corners = []
    for sign_x, sign_y in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
        rx, ry = _rotate(sign_x * half_w - center_x, sign_y * half_h - center_y, box.rotate[2])
        corners.append((pivot_x + rx, pivot_y + ry))
    return corners


def _to_local(point: tuple[float, float], result: ItemResult) -> tuple[float, float]:
    """フレーム座標の点を、result の位置・拡大率(X)・Z軸回転を原点とするローカル座標に戻す。"""
    x, y = point
    scale = result.scale[0]
    return _rotate((x - result.pos[0]) / scale, (y - result.pos[1]) / scale, -result.rotate[2])


def _compose_chain(chain: list[ItemStructure.Video]) -> tuple[float, float, float, float, float]:
    """グループの鎖(内側 → 外側)を1つの相似変換に合成し、(拡大率, 回転, X, Y, 不透明度) を返す。"""
    inner = chain[0]
    scale = inner.scale / 100.0
    rotation = inner.rotation
    tx, ty = inner.x, inner.y
    alpha = inner.alpha / 100.0
    for p in chain[1:]:
        p_scale = p.scale / 100.0
        rx, ry = _rotate(tx, ty, p.rotation)
        tx, ty = p_scale * rx + p.x, p_scale * ry + p.y
        scale *= p_scale
        rotation += p.rotation
        alpha *= p.alpha / 100.0
    return scale, rotation, tx, ty, alpha


def _apply_chain(item: ItemStructure.Video, chain: list[ItemStructure.Video]) -> ItemStructure.Video:
    """
    グループの鎖(内側 → 外側)を1つの相似変換に合成し、メンバーの位置・回転・拡大率・透明度に適用する。
    エフェクトはメンバー自身 → 内側 → 外側の順に積む。
    """
    scale, rotation, tx, ty, alpha = _compose_chain(chain)

    x, y = _rotate(item.x, item.y, rotation)
    effects = [_rebuild(e) for e in item.effects]
    for group in chain:
        group_effect_ids = {e["id"] for e in group.effects}
        effects.extend(_copy_effect(e, item.id, group_effect_ids) for e in group.effects)

    return ItemStructure.Video(
        id=item.id,
        layer=item.layer,
        start=item.start,
        end=item.end,
        min=item.min,
        max=item.max,
        x=scale * x + tx,
        y=scale * y + ty,
        scale=item.scale * scale,
        rotation=item.rotation + rotation,
        alpha=item.alpha * alpha,
        object=_rebuild(item.object),
        effects=effects,
    )


def _with_position(item: ItemStructure.Video, x: float, y: float) -> ItemStructure.Video:
    """位置だけを変えた ItemStructure.Video を返す。"""
    return ItemStructure.Video(
        id=item.id,
        layer=item.layer,
        start=item.start,
        end=item.end,
        min=item.min,
        max=item.max,
        x=x,
        y=y,
        scale=item.scale,
        rotation=item.rotation,
        alpha=item.alpha,
        object=_rebuild(item.object),
        effects=[_rebuild(e) for e in item.effects],
    )


def _copied_id(effect_id: str, item_id: str) -> str:
    """複製したエフェクトの id。フレーム間で同じ値になるよう、元の id と複製先アイテムから決める。"""
    return str(uuid.uuid5(uuid.NAMESPACE_URL, f"{effect_id}:{item_id}"))


def _copy_effect(effect: dict, item_id: str, group_effect_ids: set[str]) -> GenerateStructure:
    link_id = effect.get("link_id")
    if link_id in group_effect_ids:
        link_id = _copied_id(link_id, item_id)
    return _rebuild(effect, id=_copied_id(effect["id"], item_id), link_id=link_id)


def _rebuild(structure: dict, id: str | None = None, link_id: str | None = None) -> GenerateStructure:
    """ItemStructure から取り出した dict を、ItemStructure.Video に渡せる GenerateStructure に作り直す。"""
    return GenerateStructure(
        id=structure["id"] if id is None else id,
        name=structure["name"],
        display_name=structure["display_name"],
        parameters=dict(structure["parameters"]),
        link_id=structure.get("link_id") if id is None else link_id,
    )

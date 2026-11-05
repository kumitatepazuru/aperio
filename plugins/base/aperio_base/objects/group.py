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
            # 内側のグループから外側へたどる
            chain = [parent[item.layer]]
            while chain[-1].layer in parent:
                chain.append(parent[chain[-1].layer])
            result.append(_apply_chain(item, chain))
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
            points = _box_corners(item, result)
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
            chain = [group]
            while chain[-1].layer in parent:
                chain.append(parent[chain[-1].layer])
            if any(g.scale == 0 for g in chain):
                continue
            local = [_to_local(p, chain) for p in points]
            xs = [x for x, _ in local]
            ys = [y for _, y in local]
            # 枠の中心はグループの原点(x, y)からの相対位置。center は「原点が枠の中心からどれだけずれるか」なので符号が逆
            new_results[group.id] = ItemResult(
                _ceil_size(max(xs) - min(xs)),
                _ceil_size(max(ys) - min(ys)),
                center_x=-(max(xs) + min(xs)) / 2,
                center_y=-(max(ys) + min(ys)) / 2,
            )
        return new_results

    def generate(self, params: VideoGenerateParameters) -> ItemResult:
        # グループ自身は何も描画しない。枠の大きさはメンバーの生成後に AfterMakeFrame で決まるので、ここでは仮の値を返す
        return ItemResult(1, 1, center_x=0.0, center_y=0.0)

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


def _box_corners(item: ItemStructure.Video, result: ItemResult) -> list[tuple[float, float]]:
    """アイテムの枠の4つの角をフレーム座標で返す。UI の Overlay.tsx が枠を描く計算と同じ。"""
    box = result.offscreen_source or result
    scale_x = item.scale / 100.0 * (box.scale[0] if box.scale else 1.0)
    scale_y = item.scale / 100.0 * (box.scale[1] if box.scale else 1.0)
    pivot_x = item.x + (box.pos[0] if box.pos else 0.0)
    pivot_y = item.y + (box.pos[1] if box.pos else 0.0)
    center_x = (box.center_x or 0.0) * scale_x
    center_y = (box.center_y or 0.0) * scale_y
    half_w = box.width * scale_x / 2
    half_h = box.height * scale_y / 2

    corners = []
    for sign_x, sign_y in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
        rx, ry = _rotate(sign_x * half_w - center_x, sign_y * half_h - center_y, item.rotation)
        corners.append((pivot_x + rx, pivot_y + ry))
    return corners


def _to_local(point: tuple[float, float], chain: list[ItemStructure.Video]) -> tuple[float, float]:
    """フレーム座標の点を、鎖(内側 → 外側)の先頭のグループのローカル座標に戻す。`_apply_chain` の逆変換。"""
    x, y = point
    for g in reversed(chain):
        x, y = _rotate((x - g.x) / (g.scale / 100.0), (y - g.y) / (g.scale / 100.0), -g.rotation)
    return x, y


def _apply_chain(item: ItemStructure.Video, chain: list[ItemStructure.Video]) -> ItemStructure.Video:
    """
    グループの鎖(内側 → 外側)を1つの相似変換に合成し、メンバーの位置・回転・拡大率・透明度に適用する。
    エフェクトはメンバー自身 → 内側 → 外側の順に積む。
    """
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

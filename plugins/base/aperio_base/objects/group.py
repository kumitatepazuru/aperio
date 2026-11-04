import math
import uuid

from aperio.item_structures import (
    GenerateStructure,
    GeneratorEvent,
    GeneratorInformation,
    ItemStructure,
    RequestStructureParameter,
)
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorTextureReturn, VideoGenerateParameters, VideoObjectGeneratorBase


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

    def generate(self, params: VideoGenerateParameters) -> GeneratorTextureReturn | None:
        return None

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
            if active:
                parent[item.layer] = active[-1][0]
            if self._is_group(item):
                active.append((item, _range_end(item)))
                if not item.object["parameters"].get("affected_by_parent", False):
                    parent.pop(item.layer, None)
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

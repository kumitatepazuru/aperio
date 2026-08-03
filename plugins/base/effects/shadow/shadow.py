import struct
import uuid

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import (
    AdditionalObject,
    FileFilter,
    GeneratorEvent,
    GeneratorInformation,
    ItemResult,
    ItemStructure,
    RequestStructureParameter,
)
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ..common.deferred_object import make_deferred_video_object, rerender_owner_object
from ..common.params import clamp, make_generator_information, pack_box_average_dir_params, pack_expand_params
from ..common.pattern_image import PATTERN_EXTENSIONS, PatternImageCache
from ..common.shader_loader import compose_common_shader, effect_dirs, lib_module, shared_shader


def _clamp_axis(offset: int, size: int, r: int, max_dim: int) -> tuple[int, int]:
    """README §2 のオフセット・半径クランプ。オフセットが先に半径の予算を食う。"""
    budget = max(0, max_dim - size)
    if abs(offset) > budget:
        offset = (1 if offset >= 0 else -1) * budget
    remaining = budget - abs(offset)
    return offset, max(0, min(r, remaining // 2))


class ShadowEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.shadow_effect"
        self.display_name = "シャドー"
        self.description = "Draws a blurred, colored (or pattern-filled) copy of the object's alpha silhouette offset behind it."

        _, common_dir = effect_dirs(__file__)
        blur_module = lib_module(common_dir, "blur")

        self.expand_shader = shared_shader("expand", common_dir, "expand.wgsl")
        self.box_average_dir_shader = compose_common_shader(
            "shadow_box_average_dir", [blur_module], common_dir, "box_average_dir.wgsl"
        )
        self.select_shader = shared_shader("shadow_select", common_dir, "select.wgsl")
        self.encode_color_shader = shared_shader("encode_color", common_dir, "encode_color.wgsl")
        self.encode_pattern_shader = shared_shader("encode_pattern", common_dir, "encode_pattern.wgsl")
        self.tile_shader = shared_shader("tile", common_dir, "tile.wgsl")
        self.composite_shader = shared_shader("composite", common_dir, "composite.wgsl")

        self.pattern_cache = PatternImageCache("shadow_pattern")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, params: dict) -> GeneratorInformation:
        paths = params.get("pattern_path", [])
        if paths:
            self.pattern_cache.ensure_loaded(paths[0])

        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Int(
                    id="x",
                    title="X",
                    default_value=-40,
                    suffix="px",
                    min=-200,
                    max=200,
                ),
                RequestStructureParameter.Int(
                    id="y",
                    title="Y",
                    default_value=24,
                    suffix="px",
                    min=-200,
                    max=200,
                ),
                RequestStructureParameter.Float(
                    id="strength",
                    title="濃さ",
                    default_value=40.0,
                    suffix="%",
                    min=0.0,
                    max=100.0,
                ),
                RequestStructureParameter.Int(
                    id="diffusion",
                    title="拡散",
                    default_value=10,
                    suffix="px",
                    min=0,
                    max=50,
                ),
                RequestStructureParameter.Color(
                    id="color",
                    title="影色の設定",
                    default_value=(0.0, 0.0, 0.0, 1.0),
                    use_alpha=False,
                ),
                RequestStructureParameter.File(
                    id="pattern_path",
                    title="パターン画像ファイル",
                    multi_selections=False,
                    open_type="file",
                    filters=[FileFilter("画像ファイル", PATTERN_EXTENSIONS)],
                ),
                RequestStructureParameter.Bool(
                    id="separate_object",
                    title="影を別オブジェクトで描画",
                    default_value=False,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        strength_ui = clamp(args.get("strength", 40.0), 0.0, 100.0)

        # 濃さ=0は実機と同じく即return(README §2、画像にもキャンバスにも一切触れない)。
        strength_raw = round(strength_ui * 10)
        if strength_raw == 0:
            return None
        density = strength_raw / 1000.0

        X = int(clamp(args.get("x", -40), -200, 200))
        Y = int(clamp(args.get("y", 24), -200, 200))
        r = max(0, int(clamp(args.get("diffusion", 10), 0, 50)))
        color = args.get("color", (0.0, 0.0, 0.0, 1.0))
        pattern_paths = args.get("pattern_path", [])
        pattern_path = pattern_paths[0] if pattern_paths else ""
        separate_object = bool(args.get("separate_object", False))

        w, h = params.width, params.height
        max_dim = aperio_plugin.image_generator.maximum_texture_size

        # クランプ(README §2)。Xが先に予算を食い、Yがそれを引き継ぐ。
        X, r = _clamp_axis(X, w, r, max_dim)
        Y, r = _clamp_axis(Y, h, r, max_dim)

        # 半径の2分割(README §2)。r1<=r2、r1が最後(最終エンコード直前)のパスに使われる。
        r1 = r // 2
        r2 = r - r1
        box_w, box_h = w + 2 * r, h + 2 * r

        entry = self.pattern_cache.get(pattern_path) if pattern_path else None
        loader, pattern_func = entry if entry is not None else (None, None)

        if separate_object:
            # 別アイテムとして切り離されるため、暗黙のパイプライン合流(add_parallel_wgsl経由で
            # 前段のstateを継承する仕組み)に頼れない。owner の object をもう一度generate()して、
            # 実際にピクセルを持つ builder をシルエットの起点にする(README §7)。
            mask_start_builder = rerender_owner_object(params.layer, params.frame_number, w, h)
            if mask_start_builder is None:
                # 元オブジェクトを再生成できない場合は影を諦め、素通しのみ返す。
                identity_builder = gpu_util.PyImageGenerateBuilder().add_wgsl(self.select_shader, struct.pack("i", 0), w, h)
                return GeneratorBuilderReturn(identity_builder, ItemResult(w, h))
        else:
            mask_start_builder = gpu_util.PyImageGenerateBuilder()

        # --- 影レイヤーの生成(box_w x box_h、ストレートアルファ) ---
        mask_chain = mask_start_builder.add_wgsl(
            self.expand_shader, pack_expand_params(r, r, box_w, box_h), box_w, box_h
        )
        for radius, step_x, step_y in ((r2, 0, 1), (r2, 1, 0), (r1, 0, 1), (r1, 1, 0)):
            mask_chain = mask_chain.add_wgsl(
                self.box_average_dir_shader,
                pack_box_average_dir_params(radius, step_x, step_y, box_w, box_h),
                box_w,
                box_h,
            )

        if loader is not None and pattern_func is not None:
            tiled_branch = (
                gpu_util.PyImageGenerateBuilder()
                .add_texture_func(pattern_func, None, loader.width, loader.height)
                .add_wgsl(self.tile_shader, struct.pack("ii", box_w, box_h), box_w, box_h)
            )
            shadow_layer = (
                gpu_util.PyImageGenerateBuilder()
                .add_parallel_wgsl([mask_chain, tiled_branch])
                .add_wgsl(self.encode_pattern_shader, struct.pack("f", density), box_w, box_h)
            )
        else:
            shadow_layer = mask_chain.add_wgsl(
                self.encode_color_shader,
                struct.pack("ffff", density, color[0], color[1], color[2]),
                box_w,
                box_h,
            )

        if not separate_object:
            # --- 通常合成(README §3) ---
            nw, nh = w + abs(X) + 2 * r, h + abs(Y) + 2 * r
            shadow_box_x, shadow_box_y = max(X, 0), max(Y, 0)
            obj_x, obj_y = max(-X, 0) + r, max(-Y, 0) + r

            shadow_full_branch = shadow_layer.add_wgsl(
                self.expand_shader, pack_expand_params(shadow_box_x, shadow_box_y, nw, nh), nw, nh
            )
            object_full_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(
                self.expand_shader, pack_expand_params(obj_x, obj_y, nw, nh), nw, nh
            )
            final_builder = (
                gpu_util.PyImageGenerateBuilder()
                .add_parallel_wgsl([object_full_branch, shadow_full_branch])
                .add_wgsl(self.composite_shader, None, nw, nh)
            )
            # キャンバスは|X|/|Y|ぶん片側にだけ伸びる(2rは対称)ので、オブジェクト自身の
            # 見た目の位置・回転/拡縮の基点(compose.wgslのcenter_x/center_y)が
            # ずれないよう半分だけ戻す(README §3「fpip+0xD4/+0xD8の補正」に相当)。
            center_x = round(-X / 2)
            center_y = round(-Y / 2)
            return GeneratorBuilderReturn(final_builder, ItemResult(nw, nh, center_x=center_x, center_y=center_y))

        # --- 影を別オブジェクトで描画(README §7) ---
        # 元のオブジェクトは1バイトも変わらずに素通しする。
        owner = params.layer
        shadow_object_structure = make_deferred_video_object(GeneratorBuilderReturn(shadow_layer, ItemResult(box_w, box_h)))
        additional_item = ItemStructure.Video(
            id=str(uuid.uuid4()),
            layer=owner.layer,
            start=owner.start,
            end=owner.end,
            min=None,
            max=None,
            x=owner.x + X,
            y=owner.y + Y,
            scale=owner.scale,
            rotation=owner.rotation,
            alpha=owner.alpha,
            object=shadow_object_structure,
            effects=[],
        )
        additional_object = AdditionalObject(item=additional_item, behind=True)

        identity_builder = gpu_util.PyImageGenerateBuilder().add_wgsl(self.select_shader, struct.pack("i", 0), w, h)
        return GeneratorBuilderReturn(identity_builder, ItemResult(w, h, additional_object=additional_object))

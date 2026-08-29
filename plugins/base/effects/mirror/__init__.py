import struct

import aperio_plugin
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.shader_loader import effect_dirs, shared_shader

# axis: 0=垂直方向(上下、対象寸法=h)、1=水平方向(左右、対象寸法=w)
# before: 鏡像を原画像より前(上/左)に置くか、後(下/右)に置くか
_DIR_INFO = {
    "up": (0, True),
    "down": (0, False),
    "left": (1, True),
    "right": (1, False),
}


class MirrorEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.mirror_effect"
        self.display_name = "ミラー"
        self.description = "Grows the canvas and draws a fading mirrored copy of the object next to it."

        current_dir, _ = effect_dirs(__file__)
        self.mirror_shader = shared_shader("mirror", current_dir, "mirror.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.List(
                    id="direction",
                    title="ミラーの方向",
                    values={"up": "上側", "down": "下側", "left": "左側", "right": "右側"},
                    default_value="down",
                ),
                RequestStructureParameter.Float(
                    id="transparency", title="透明度", default_value=0.0, suffix="%", min=0.0, max=100.0
                ),
                RequestStructureParameter.Float(
                    id="falloff", title="減衰", default_value=0.0, suffix="%", min=0.0, max=500.0
                ),
                RequestStructureParameter.Int(
                    id="seam_offset", title="境目調整", default_value=0, suffix="px", min=-2000, max=2000
                ),
                RequestStructureParameter.Bool(
                    id="recenter", title="中心の位置を変更", default_value=False
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn | None:
        args = params.args
        axis, before = _DIR_INFO.get(args.get("direction", "down"), _DIR_INFO["down"])
        transparency = float(args.get("transparency", 0.0))
        falloff = float(args.get("falloff", 0.0))
        seam_offset = int(args.get("seam_offset", 0))
        recenter = bool(args.get("recenter", False))

        w, h = params.width, params.height
        dim = h if axis == 0 else w
        max_dim = aperio_plugin.image_generator.maximum_texture_size
        if 2 * dim > max_dim:
            return None

        gap = max(0, seam_offset) * 2
        crop = max(0, -seam_offset)
        mirror_len = max(0, dim - crop)
        max_extra = max_dim - dim
        if gap + mirror_len > max_extra:
            gap = max(0, min(gap, max_extra))
            mirror_len = max(0, max_extra - gap)
        total_axis_dim = dim + gap + mirror_len

        if before:
            mirror_start, orig_start = 0, mirror_len + gap
            mirror_a, dist_c, dist_slope = mirror_len - 1, mirror_len - 1, -1
        else:
            orig_start, mirror_start = 0, dim + gap
            mirror_a, dist_c, dist_slope = dim - 1 + mirror_start, -mirror_start, 1

        ow, oh = (w, total_axis_dim) if axis == 0 else (total_axis_dim, h)
        base_opacity = 1.0 - transparency / 100.0
        frame_state = aperio_plugin.store_manager.get_state().frame_state
        frame_dim = frame_state.height if axis == 0 else frame_state.width
        falloff_coef = (falloff / 100.0) / max(1, frame_dim)

        shader_params = struct.pack(
            "iiiiiiiiffii",
            axis, orig_start, dim, mirror_start, mirror_len,
            mirror_a, dist_c, dist_slope,
            base_opacity, falloff_coef, ow, oh,
        )

        if recenter:
            item_result = ItemResult(ow, oh)
        else:
            center_axis = (orig_start + dim // 2) - total_axis_dim // 2
            item_result = ItemResult(
                ow, oh,
                center_x=(center_axis if axis == 1 else 0),
                center_y=(center_axis if axis == 0 else 0),
            )
        return GeneratorWgslReturn(self.mirror_shader, shader_params, item_result)

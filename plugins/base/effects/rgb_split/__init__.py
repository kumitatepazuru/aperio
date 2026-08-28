import math
import struct

import aperio_plugin
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.shader_loader import compose_common_shader, effect_dirs, lib_module

_SPLIT_TYPE_INDEX = {
    "red_green_a": 0,
    "red_blue_a": 1,
    "green_blue_a": 2,
    "red_green_b": 3,
    "red_blue_b": 4,
    "green_blue_b": 5,
}


class RgbSplitEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.rgb_split_effect"
        self.display_name = "色ずれ"
        self.description = "Shifts two of the R/G/B channels in opposite directions, crossfaded with the original."

        current_dir, common_dir = effect_dirs(__file__)
        math_module = lib_module(common_dir, "math")
        self.rgb_split_shader = compose_common_shader(
            "rgb_split", [math_module], current_dir, "rgb_split.wgsl"
        )

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Float(
                    id="shift_width", title="ずれ幅", default_value=5.0, suffix="px", min=0.0, max=2000.0
                ),
                RequestStructureParameter.Float(
                    id="angle", title="角度", default_value=0.0, suffix="°"
                ),
                RequestStructureParameter.Float(
                    id="strength", title="強さ", default_value=100.0, suffix="%", min=0.0, max=100.0
                ),
                RequestStructureParameter.List(
                    id="split_type",
                    title="色ずれの種類",
                    values={
                        "red_green_a": "赤緑A",
                        "red_blue_a": "赤青A",
                        "green_blue_a": "緑青A",
                        "red_green_b": "赤緑B",
                        "red_blue_b": "赤青B",
                        "green_blue_b": "緑青B",
                    },
                    default_value="red_green_a",
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn | None:
        args = params.args
        shift_width = max(0.0, float(args.get("shift_width", 5.0)))
        angle = float(args.get("angle", 0.0))
        strength = max(0.0, min(100.0, float(args.get("strength", 100.0)))) / 100.0
        split_type = _SPLIT_TYPE_INDEX.get(args.get("split_type", "red_green_a"), 0)
        pair = split_type % 3
        variant = split_type // 3

        if shift_width == 0.0:
            return None

        # exedit-inspect rgb_split README: 0度=上, 正=時計回り、四捨五入で整数pxへ。
        theta = math.radians(angle)
        dx = round(-math.sin(theta) * shift_width)
        dy = round(math.cos(theta) * shift_width)

        w, h = params.width, params.height
        if dx == 0 and dy == 0:
            return None

        max_dim = aperio_plugin.image_generator.maximum_texture_size
        x0, x1 = self._grow_axis(w, abs(dx), max_dim)
        y0, y1 = self._grow_axis(h, abs(dy), max_dim)
        ow, oh = x1 - x0, y1 - y0

        shader_params = struct.pack(
            "iiiifiiii", dx, dy, pair, variant, strength, -x0, -y0, ow, oh
        )

        center_x = w // 2 - (x0 + ow // 2)
        center_y = h // 2 - (y0 + oh // 2)
        return GeneratorWgslReturn(
            self.rgb_split_shader, shader_params, ItemResult(ow, oh, center_x=center_x, center_y=center_y)
        )

    def _grow_axis(self, dim: int, range_px: int, max_dim: int) -> tuple[int, int]:
        lo, hi = -range_px, dim + range_px
        lim = min(max_dim, dim + 2 * range_px)
        trim_lo = True
        while hi - lo > lim:
            if trim_lo:
                lo += 1
            else:
                hi -= 1
            trim_lo = not trim_lo
        return lo, hi

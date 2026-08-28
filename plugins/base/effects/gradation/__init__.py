import math
import struct

from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.shader_loader import compose_common_shader, effect_dirs, lib_module

_SHAPE_INDEX = {"line": 0, "circle": 1, "rectangle": 2, "convex": 3}
_BLEND_INDEX = {
    "normal": 0,
    "add": 1,
    "subtract": 2,
    "multiply": 3,
    "screen": 4,
    "overlay": 5,
    "lighten": 6,
    "darken": 7,
    "luminance": 8,
    "chroma": 9,
    "shade": 10,
    "chiaroscuro": 11,
    "difference": 12,
}


class GradationEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.gradation_effect"
        self.display_name = "グラデーション"
        self.description = "Fills the frame with a 2-color gradient (line/circle/rectangle/convex) blended onto the object below."

        current_dir, common_dir = effect_dirs(__file__)
        color_module = lib_module(common_dir, "color")
        self.gradation_shader = compose_common_shader(
            "gradation", [color_module], current_dir, "gradation.wgsl"
        )

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Float(
                    id="strength", title="強さ", default_value=100.0, suffix="%", min=0.0, max=100.0
                ),
                RequestStructureParameter.Vec2Int(
                    id="center", title="中心", default_value=(0, 0), suffix="px", min=-2000, max=2000
                ),
                RequestStructureParameter.Float(
                    id="angle", title="角度", default_value=0.0, suffix="°"
                ),
                RequestStructureParameter.Float(
                    id="width", title="幅", default_value=100.0, suffix="px", min=0.0, max=2000.0
                ),
                RequestStructureParameter.List(
                    id="shape",
                    title="形状",
                    values={"line": "線", "circle": "円", "rectangle": "四角形", "convex": "凸形"},
                    default_value="line",
                ),
                RequestStructureParameter.Color(
                    id="start_color", title="開始色", default_value=(1.0, 1.0, 1.0, 1.0), use_alpha=True
                ),
                RequestStructureParameter.Color(
                    id="end_color", title="終了色", default_value=(0.0, 0.0, 0.0, 1.0), use_alpha=True
                ),
                RequestStructureParameter.List(
                    id="blend_mode",
                    title="合成",
                    values={
                        "normal": "通常",
                        "add": "加算",
                        "subtract": "減算",
                        "multiply": "乗算",
                        "screen": "スクリーン",
                        "overlay": "オーバーレイ",
                        "lighten": "比較(明)",
                        "darken": "比較(暗)",
                        "luminance": "輝度",
                        "chroma": "色差",
                        "shade": "陰影",
                        "chiaroscuro": "明暗",
                        "difference": "差分",
                    },
                    default_value="normal",
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn | None:
        args = params.args
        strength = max(0.0, min(100.0, float(args.get("strength", 100.0)))) / 100.0
        center = args.get("center", (0, 0))
        angle = float(args.get("angle", 0.0))
        width = max(0.0, float(args.get("width", 100.0)))
        shape = _SHAPE_INDEX.get(args.get("shape", "line"), 0)
        start_color = args.get("start_color", (1.0, 1.0, 1.0, 1.0))
        end_color = args.get("end_color", (0.0, 0.0, 0.0, 1.0))
        blend_mode = _BLEND_INDEX.get(args.get("blend_mode", "normal"), 0)

        if strength <= 0.0:
            return None

        w, h = params.width, params.height
        cx = w / 2.0 + center[0]
        cy = h / 2.0 + center[1]
        theta = math.radians(angle)
        vx, vy = math.sin(theta), -math.cos(theta)

        # exedit-inspect gradation README §5: 線形状だけ幅を常に+1する
        # (他の3形状は幅0のとき正しく単色になるが、線は幅0だと急峻な1pxの
        # ランプになるべきで、単色に潰れてはいけない)。
        shader_width = width + 1.0 if shape == 0 else width

        shader_params = struct.pack(
            "fffffi",
            cx, cy, vx, vy, shader_width, shape,
        ) + struct.pack(
            "ffff",
            start_color[0], start_color[1], start_color[2], start_color[3] * strength,
        ) + struct.pack(
            "ffff",
            end_color[0], end_color[1], end_color[2], end_color[3] * strength,
        ) + struct.pack(
            "iii",
            blend_mode, w, h,
        )

        return GeneratorWgslReturn(self.gradation_shader, shader_params, ItemResult(w, h))

import math
import struct

import aperio_plugin
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.shader_loader import effect_dirs, shared_shader


class PolarCoordinateTransformEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.polar_coordinate_transform_effect"
        self.display_name = "極座標変換"
        self.description = "Wraps the object into a ring: source rows become concentric circles, source columns become radial lines."

        current_dir, _ = effect_dirs(__file__)
        self.shader = shared_shader("polar_coordinate_transform", current_dir, "polar_coordinate_transform.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Int(
                    id="center_hole", title="中心幅", default_value=0, suffix="px", min=0, max=2000
                ),
                RequestStructureParameter.Float(
                    id="scale_percent", title="拡大率", default_value=100.0, suffix="%", min=100.0, max=800.0
                ),
                RequestStructureParameter.Float(
                    id="rotation", title="回転", default_value=0.0, suffix="°", min=-3600.0, max=3600.0
                ),
                RequestStructureParameter.Float(
                    id="swirl", title="渦巻", default_value=0.0, min=-8.0, max=8.0
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn | None:
        args = params.args
        center_hole = max(0, int(args.get("center_hole", 0)))
        scale_percent = max(100.0, float(args.get("scale_percent", 100.0)))
        rotation_rad = math.radians(float(args.get("rotation", 0.0)) % 360.0)
        swirl = float(args.get("swirl", 0.0))

        w, h = params.width, params.height
        r_base = (w + h) / math.pi * (scale_percent / 100.0)
        max_dim = aperio_plugin.image_generator.maximum_texture_size
        r = min(r_base + center_hole, max_dim / 2.0)
        if center_hole >= r or r <= 0:
            return None

        s = max(1, round(2 * r))
        swirl_rate = swirl * 2.0 * math.pi / max(1e-3, r - center_hole)

        shader_params = struct.pack("ifffii", center_hole, r, rotation_rad, swirl_rate, s, s)
        return GeneratorWgslReturn(
            self.shader, shader_params, ItemResult(s, s)
        )

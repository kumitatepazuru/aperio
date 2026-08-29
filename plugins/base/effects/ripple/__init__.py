import struct

import aperio_plugin
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.shader_loader import compose_common_shader, effect_dirs, lib_module


class RippleEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.ripple_effect"
        self.display_name = "波紋"
        self.description = "Displaces pixels along concentric rings expanding from a center point."

        current_dir, common_dir = effect_dirs(__file__)
        math_module = lib_module(common_dir, "math")
        self.ripple_shader = compose_common_shader(
            "ripple", [math_module], current_dir, "ripple.wgsl",
        )

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Float(
                    id="center_x", title="中心X", default_value=0.0, suffix="px", min=-5000.0, max=5000.0
                ),
                RequestStructureParameter.Float(
                    id="center_y", title="中心Y", default_value=0.0, suffix="px", min=-5000.0, max=5000.0
                ),
                RequestStructureParameter.Float(
                    id="ring_spacing", title="幅", default_value=30.0, suffix="px", min=1.0, max=400.0
                ),
                RequestStructureParameter.Float(
                    id="max_displacement", title="高さ", default_value=15.0, suffix="px", min=-400.0, max=400.0
                ),
                RequestStructureParameter.Float(
                    id="speed", title="速度", default_value=150.0, suffix="px/s", min=-2000.0, max=2000.0
                ),
                # TODO: 以下のパラメータ3つは詳細設定というボタンかアコーディオンかを作れるようにして、そこに格納するようにする
                RequestStructureParameter.Int(
                    id="ring_count", title="波紋数", default_value=0, min=0, max=1000
                ),
                RequestStructureParameter.Int(
                    id="ring_interval", title="波紋間隔", default_value=0, min=0, max=100
                ),
                RequestStructureParameter.Int(
                    id="ramp_count", title="増幅減衰回数", default_value=0, min=-1000, max=1000
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn:
        args = params.args
        center_x = float(args.get("center_x", 0.0))
        center_y = float(args.get("center_y", 0.0))
        ring_spacing = max(1.0, float(args.get("ring_spacing", 30.0)))
        max_displacement = float(args.get("max_displacement", 15.0))
        speed = float(args.get("speed", 150.0))
        ring_count = max(0, int(args.get("ring_count", 0)))
        ring_interval = max(0, int(args.get("ring_interval", 0)))
        ramp_count = int(args.get("ramp_count", 0))

        w, h = params.width, params.height
        fps = aperio_plugin.store_manager.get_state().frame_state.fps
        elapsed_seconds = (params.frame_number - params.layer.start) / fps
        wavefront = speed * elapsed_seconds

        shader_params = struct.pack(
            "fffffiiiii",
            center_x, center_y, ring_spacing, max_displacement,
            wavefront, ring_count, ring_interval, ramp_count, w, h,
        )
        return GeneratorWgslReturn(self.ripple_shader, shader_params, ItemResult(w, h))

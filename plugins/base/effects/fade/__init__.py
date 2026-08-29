import struct

from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.shader_loader import effect_dirs, shared_shader
from ...common.timeline import in_out_ramp


class FadeEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.fade_effect"
        self.display_name = "フェード"
        self.description = "Fades the object's alpha in/out near the start/end of its duration."

        current_dir, _ = effect_dirs(__file__)
        self.fade_shader = shared_shader("fade", current_dir, "fade.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Float(
                    id="fade_in",
                    title="イン",
                    default_value=0.5,
                    suffix="s",
                    min=0.0,
                    max=10.0,
                ),
                RequestStructureParameter.Float(
                    id="fade_out",
                    title="アウト",
                    default_value=0.5,
                    suffix="s",
                    min=0.0,
                    max=10.0,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn | None:
        args = params.args
        fade_in = args.get("fade_in", 0.5)
        fade_out = args.get("fade_out", 0.5)

        # README §3/§4 のランプ(`ワイプ`と共通、common/timeline.py)。フェードは
        # どちら側のランプが勝ったかを見ないので `from_out` は捨てる。
        g, _ = in_out_ramp(params, fade_in, fade_out)

        if g >= 1.0:
            return None

        shader_params = struct.pack("f", g)
        return GeneratorWgslReturn(self.fade_shader, shader_params, ItemResult(params.width, params.height))

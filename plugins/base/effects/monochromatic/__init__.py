import struct

from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.shader_loader import compose_common_shader, effect_dirs, lib_module


class MonochromaticEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.monochromatic_effect"
        self.display_name = "単色化"
        self.description = "Pulls the image's chroma (and optionally luminance) toward a single target color."

        current_dir, common_dir = effect_dirs(__file__)
        color_module = lib_module(common_dir, "color")
        self.monochromatic_shader = compose_common_shader(
            "monochromatic", [color_module], current_dir, "monochromatic.wgsl"
        )

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Float(
                    id="strength",
                    title="強さ",
                    default_value=100.0,
                    suffix="%",
                    min=0.0,
                    max=100.0,
                ),
                RequestStructureParameter.Color(
                    id="color",
                    title="色の設定",
                    default_value=(0.5, 0.5, 0.5, 1.0),
                    use_alpha=False,
                ),
                RequestStructureParameter.Bool(
                    id="preserve_luminance",
                    title="輝度を保持する",
                    default_value=True,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn:
        args = params.args
        strength = max(0.0, min(100.0, float(args.get("strength", 100.0)))) / 100.0
        color = args.get("color", (0.5, 0.5, 0.5, 1.0))
        preserve_luminance = bool(args.get("preserve_luminance", True))

        shader_params = struct.pack(
            "fifff", strength, int(preserve_luminance), color[0], color[1], color[2]
        )

        return GeneratorWgslReturn(
            self.monochromatic_shader, shader_params, ItemResult(params.width, params.height)
        )

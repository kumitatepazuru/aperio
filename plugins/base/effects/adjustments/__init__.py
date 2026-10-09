import struct

from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorShaderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.shader_loader import effect_dirs, shared_slang_shader


class ColorAdjustmentEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.color_adjustment"
        self.display_name = "色調補正"
        self.description = "Adjusts brightness, contrast, hue, luminance, and saturation."

        current_dir, _ = effect_dirs(__file__)

        self.color_shader = shared_slang_shader("color", current_dir, "color.slang")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Int(
                    id="brightness",
                    title="明るさ",
                    default_value=100,
                    suffix="",
                    min=0,
                    max=200,
                ),
                RequestStructureParameter.Int(
                    id="contrast",
                    title="コントラスト",
                    default_value=100,
                    suffix="",
                    min=0,
                    max=200,
                ),
                RequestStructureParameter.Int(
                    id="hue",
                    title="色相",
                    default_value=0,
                    suffix="°",
                    min=-360,
                    max=360,
                ),
                RequestStructureParameter.Int(
                    id="luminance",
                    title="輝度",
                    default_value=100,
                    suffix="",
                    min=0,
                    max=200,
                ),
                RequestStructureParameter.Int(
                    id="saturation",
                    title="彩度",
                    default_value=100,
                    suffix="",
                    min=0,
                    max=200,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorShaderReturn:
        args = params.args
        brightness = args.get("brightness", 100)
        contrast = args.get("contrast", 100)
        hue = args.get("hue", 0)
        luminance = args.get("luminance", 100)
        saturation = args.get("saturation", 100)

        shader_params = struct.pack(
            "fffff",
            float(brightness),
            float(contrast),
            float(hue),
            float(luminance),
            float(saturation),
        )

        return GeneratorShaderReturn(self.color_shader, shader_params, ItemResult(params.width, params.height))

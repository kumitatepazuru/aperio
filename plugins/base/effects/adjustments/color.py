import os
import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio.gpu_util import PyCompiledWgsl
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters


class ColorAdjustmentEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.color_adjustment"
        self.display_name = "色調補正"
        self.description = "Adjusts brightness, contrast, hue, luminance, and saturation."

        current_dir = os.path.dirname(__file__)
        common_dir = os.path.join(current_dir, "..", "common")
        color_module = gpu_util.create_composable_module(os.path.join(common_dir, "lib", "color.wgsl"))

        self.color_shader = PyCompiledWgsl.compose_new(
            "color",
            [color_module],
            gpu_util.create_naga_module(os.path.join(current_dir, "color.wgsl")),
            aperio_plugin.image_generator,
        )

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return GeneratorInformation(
            display_name=self.display_name,
            duration_frames=None,
            max_frame=None,
            min_frame=None,
            structure=[
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

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn:
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

        return GeneratorWgslReturn(self.color_shader, shader_params, ItemResult(params.width, params.height))

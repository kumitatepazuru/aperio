import os
import struct

import aperio_plugin
from aperio.frame_structure import GeneratorEvent, GeneratorInformation, RequestStructureParameter
from aperio.gpu_util import PyCompiledWgsl
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import EffectGeneratorBase, GenerateParameters, GeneratorWgslReturn


class BlurEffect(EffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.blur_effect"
        self.display_name = "ブラー"
        self.description = "Applies a blur effect to the input frame."

        current_dir = os.path.dirname(__file__)
        with open(os.path.join(current_dir, "blur.wgsl"), "r") as f:
            self.shader = PyCompiledWgsl("blur", f.read(), aperio_plugin.image_generator, None)

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
                    id="blur_radius",
                    title="強さ",
                    default_value=5,
                    suffix="px",
                )
            ],
        )

    def generate(self, params: GenerateParameters) -> GeneratorWgslReturn:
        args = params.args
        blur_radius = args.get("blur_radius", 5)
        if blur_radius < 0:
            blur_radius = 0
            new_width = params.width
            new_height = params.height
        else:
            new_width = params.width + 2 * blur_radius
            new_height = params.height + 2 * blur_radius
        shader_params = struct.pack("iii", blur_radius, new_width, new_height)

        return GeneratorWgslReturn(self.shader, shader_params, new_width, new_height)

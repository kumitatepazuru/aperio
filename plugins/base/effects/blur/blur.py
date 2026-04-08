import os
import struct

from aperio_plugin.plugin_base.generator_base import EffectGeneratorBase, GeneratorWgslReturn, NewEffectGeneratorReturn
from aperio_plugin.types.frame_structure import IntParam, RequestStructureParameter
from gpu_util import PyCompiledWgsl, PyImageGenerator


class BlurEffect(EffectGeneratorBase):
    def __init__(self, generator: PyImageGenerator) -> None:
        super().__init__(generator)
        self.name = "base.blur_effect"
        self.display_name = "ブラー"
        self.description = "Applies a blur effect to the input frame."

        current_dir = os.path.dirname(__file__)
        with open(os.path.join(current_dir, "blur.wgsl"), "r") as f:
            self.shader = PyCompiledWgsl("blur", f.read(), generator, None)

    def on_new(self) -> NewEffectGeneratorReturn:
        return NewEffectGeneratorReturn(
            display_name=self.display_name,
            structure=[
                IntParam(
                    id="blur_radius",
                    title="強さ",
                    default_value=5,
                    suffix="px"
                )
            ]
        )

    def on_request_structure(self, params: dict) -> list[RequestStructureParameter]:
        return [
                IntParam(
                    id="blur_radius",
                    title="強さ",
                    default_value=5,
                    suffix="px"
                )
            ]

    def generate(self, frame_number: int, args: dict, width: int, height: int) -> GeneratorWgslReturn:
        blur_radius = args.get("blur_radius", 5)
        # ぼかしにより各辺 radius 分ずつ拡張
        if blur_radius < 0:
            blur_radius = 0
            new_width = width
            new_height = height
        else:
            new_width = width + 2 * blur_radius
            new_height = height + 2 * blur_radius
        params = struct.pack("iii", blur_radius, new_width, new_height)

        return GeneratorWgslReturn(self.shader, params, new_width, new_height)
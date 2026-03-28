import os
import struct

from aperio_plugin.plugin_base.generator_base import FilterGeneratorBase, GeneratorWgslReturn, NewFilterGeneratorReturn
from aperio_plugin.types.frame_structure import IntParam, RequestStructureParameter
from gpu_util import PyCompiledWgsl, PyImageGenerator


class BlurFilter(FilterGeneratorBase):
    def __init__(self, generator: PyImageGenerator) -> None:
        super().__init__(generator)
        self.name = "base.blur_filter"
        self.display_name = "Blur Filter"
        self.description = "Applies a blur effect to the input frame."

        current_dir = os.path.dirname(__file__)
        with open(os.path.join(current_dir, "blur.wgsl"), "r") as f:
            self.shader = PyCompiledWgsl("blur", f.read(), generator, None)

    def on_new(self) -> NewFilterGeneratorReturn:
        return NewFilterGeneratorReturn(
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
        blur_radius = args.get("blur_radius", 5.0)
        params = struct.pack("i", blur_radius)

        # ぼかしにより各辺 radius 分ずつ拡張
        new_width = width + 2 * blur_radius
        new_height = height + 2 * blur_radius

        return GeneratorWgslReturn(self.shader, params, new_width, new_height)
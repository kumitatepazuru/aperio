import os
import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio.gpu_util import PyCompiledWgsl
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters


class BlurEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.blur_effect"
        self.display_name = "ブラー"
        self.description = "Applies a blur effect to the input frame."

        current_dir = os.path.dirname(__file__)
        with open(os.path.join(current_dir, "blur_h.wgsl"), "r") as f:
            self.blur_h_shader = PyCompiledWgsl("blur_h", f.read(), aperio_plugin.image_generator, None)
        with open(os.path.join(current_dir, "blur_v.wgsl"), "r") as f:
            self.blur_v_shader = PyCompiledWgsl("blur_v", f.read(), aperio_plugin.image_generator, None)

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
                    min=0,
                )
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn:
        args = params.args
        blur_radius = max(0, args.get("blur_radius", 5))

        new_width = params.width + 2 * blur_radius
        new_height = params.height + 2 * blur_radius
        inter_width = new_width    # 水平パス後: 幅は最終サイズ、高さは元のまま
        inter_height = params.height

        h_params = struct.pack("iii", blur_radius, inter_width, inter_height)
        v_params = struct.pack("iii", blur_radius, new_width, new_height)

        builder = (
            gpu_util.PyImageGenerateBuilder()
            .add_wgsl(self.blur_h_shader, h_params, inter_width, inter_height)
            .add_wgsl(self.blur_v_shader, v_params, new_width, new_height)
        )

        return GeneratorBuilderReturn(builder, ItemResult(new_width, new_height))

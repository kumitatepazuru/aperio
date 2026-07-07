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
        self.display_name = "ぼかし"
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
                ),
                RequestStructureParameter.Int(
                    id="aspect",
                    title="縦横比",
                    default_value=0,
                    min=-100,
                    max=100,
                ),
                RequestStructureParameter.Int(
                    id="light_intensity",
                    title="光の強さ",
                    default_value=0,
                    min=0,
                    max=60,
                ),
                RequestStructureParameter.Bool(
                    id="fixed_size",
                    title="サイズ固定",
                    default_value=False,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        blur_radius = max(0, args.get("blur_radius", 5))
        aspect = max(-100, min(100, args.get("aspect", 0)))
        light_intensity = max(0, min(60, args.get("light_intensity", 0)))
        fixed_size = bool(args.get("fixed_size", False))
        fixed_size_int = 1 if fixed_size else 0

        # aspect > 0: 横方向のみに近づく → 縦半径を縮小
        # aspect < 0: 縦方向のみに近づく → 横半径を縮小
        if aspect >= 0:
            h_radius = blur_radius
            v_radius = int(blur_radius * (1 - aspect / 100))
        else:
            h_radius = int(blur_radius * (1 + aspect / 100))
            v_radius = blur_radius

        if h_radius == 0 and v_radius == 0:
            return None

        if fixed_size:
            new_width = params.width
            new_height = params.height
        else:
            new_width = params.width + 2 * h_radius
            new_height = params.height + 2 * v_radius
        inter_width = new_width
        inter_height = params.height

        h_params = struct.pack("iiiii", h_radius, inter_width, inter_height, light_intensity, fixed_size_int)
        v_params = struct.pack("iiiii", v_radius, new_width, new_height, light_intensity, fixed_size_int)

        builder = (
            gpu_util.PyImageGenerateBuilder()
            .add_wgsl(self.blur_h_shader, h_params, inter_width, inter_height)
            .add_wgsl(self.blur_v_shader, v_params, new_width, new_height)
        )

        return GeneratorBuilderReturn(builder, ItemResult(new_width, new_height))

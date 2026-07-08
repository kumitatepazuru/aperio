import os
import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio.gpu_util import PyCompiledWgsl
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters


class MozaicEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.mozaic_effect"
        self.display_name = "モザイク"
        self.description = "Applies a mosaic effect to the input frame, aligned to the center."

        current_dir = os.path.dirname(__file__)
        with open(os.path.join(current_dir, "mozaic_h.wgsl"), "r") as f:
            self.mozaic_h_shader = PyCompiledWgsl("mozaic_h", f.read(), aperio_plugin.image_generator, None)
        with open(os.path.join(current_dir, "mozaic_v.wgsl"), "r") as f:
            self.mozaic_v_shader = PyCompiledWgsl("mozaic_v", f.read(), aperio_plugin.image_generator, None)

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
                    id="size",
                    title="サイズ",
                    default_value=10,
                    suffix="px",
                    min=1,
                ),
                RequestStructureParameter.Bool(
                    id="tile",
                    title="タイル風",
                    default_value=False,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        size = max(1, args.get("size", 10))
        tile = bool(args.get("tile", False))

        width, height = params.width, params.height

        # 中心を基準にブロックを区切るため、割り切れない端数は上下左右の端で切り取られる
        center_x = width // 2
        center_y = height // 2

        h_params = struct.pack("iiii", size, center_x, width, height)
        v_params = struct.pack("iiiiii", size, center_x, center_y, width, height, 1 if tile else 0)

        builder = (
            gpu_util.PyImageGenerateBuilder()
            .add_wgsl(self.mozaic_h_shader, h_params, width, height)
            .add_wgsl(self.mozaic_v_shader, v_params, width, height)
        )

        return GeneratorBuilderReturn(builder, ItemResult(width, height))

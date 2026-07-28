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
        common_dir = os.path.join(current_dir, "..", "common")
        color_module = gpu_util.create_composable_module(os.path.join(common_dir, "lib", "color.wgsl"))
        math_module = gpu_util.create_composable_module(os.path.join(common_dir, "lib", "math.wgsl"))

        self.mozaic_h_shader = PyCompiledWgsl.compose_new(
            "mozaic_h",
            [math_module],
            gpu_util.create_naga_module(os.path.join(current_dir, "mozaic_h.wgsl")),
            aperio_plugin.image_generator,
        )
        self.mozaic_v_shader = PyCompiledWgsl.compose_new(
            "mozaic_v",
            [math_module, color_module],
            gpu_util.create_naga_module(os.path.join(current_dir, "mozaic_v.wgsl")),
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
                    id="size",
                    title="サイズ",
                    default_value=12,
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
        size = max(1, args.get("size", 12))
        tile = bool(args.get("tile", False))

        # サイズ2未満は完全な無処理(実機は画像に触れずにTRUEを返す)。
        if size < 2:
            return None

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

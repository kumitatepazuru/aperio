import math
import os
import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio.gpu_util import PyCompiledWgsl
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

# 拡散を2つのボックスぼかし半径に分割する係数。1/2.236(≒1/√5)そのもの。
# r2 = trunc(拡散*係数), r1 = 拡散 - r2 で、常に r1 >= r2 になる。
_RADIUS_SPLIT_FACTOR = 1 / math.sqrt(5)


def _split_diffusion(diffusion: int) -> tuple[int, int]:
    r2 = int(diffusion * _RADIUS_SPLIT_FACTOR)
    r1 = diffusion - r2
    return r1, r2


class DiffusionLightEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.diffusion_light_effect"
        self.display_name = "拡散光"
        self.description = "Diffuses the image with a two-pass box blur and screens the brightened result back over the source."

        current_dir = os.path.dirname(__file__)
        common_dir = os.path.join(current_dir, "..", "common")

        def load(path: str) -> str:
            with open(path, "r") as f:
                return f.read()

        self.box_blur_h_shader = PyCompiledWgsl(
            "box_blur_h", load(os.path.join(common_dir, "box_blur_h.wgsl")), aperio_plugin.image_generator, None
        )
        self.box_blur_v_shader = PyCompiledWgsl(
            "box_blur_v", load(os.path.join(common_dir, "box_blur_v.wgsl")), aperio_plugin.image_generator, None
        )
        self.expand_shader = PyCompiledWgsl(
            "expand", load(os.path.join(common_dir, "expand.wgsl")), aperio_plugin.image_generator, None
        )
        self.ycbcr_encode_shader = PyCompiledWgsl(
            "ycbcr_encode", load(os.path.join(common_dir, "ycbcr_encode.wgsl")), aperio_plugin.image_generator, None
        )
        self.ycbcr_decode_shader = PyCompiledWgsl(
            "ycbcr_decode", load(os.path.join(common_dir, "ycbcr_decode.wgsl")), aperio_plugin.image_generator, None
        )
        self.composite_shader = PyCompiledWgsl(
            "diffusion_light_composite", load(os.path.join(current_dir, "composite.wgsl")), aperio_plugin.image_generator, None
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
                    id="strength",
                    title="強さ",
                    default_value=50,
                    min=0,
                    max=100,
                ),
                RequestStructureParameter.Int(
                    id="diffusion",
                    title="拡散",
                    default_value=12,
                    suffix="px",
                    min=0,
                    max=500,
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
        strength_ui = max(0, min(100, args.get("strength", 50)))
        diffusion = max(0, min(500, args.get("diffusion", 12)))
        fixed_size = bool(args.get("fixed_size", False))

        if strength_ui == 0:
            return None

        r1, r2 = _split_diffusion(diffusion)
        if r1 == 0:
            return None

        strength = strength_ui / 100.0

        cur_w, cur_h = params.width, params.height
        current = gpu_util.PyImageGenerateBuilder().add_wgsl(self.ycbcr_encode_shader, None, cur_w, cur_h)

        for radius in (r1, r2):
            if radius <= 0:
                continue

            offset = 0 if fixed_size else radius
            new_w = cur_w if fixed_size else cur_w + 2 * radius
            new_h = cur_h if fixed_size else cur_h + 2 * radius
            border_mode = 1
            divisor_mode = 1 if fixed_size else 0

            blur_branch = (
                gpu_util.PyImageGenerateBuilder()
                .add_wgsl(
                    self.box_blur_v_shader,
                    struct.pack("iiiiii", radius, cur_w, new_h, offset, border_mode, divisor_mode),
                    cur_w,
                    new_h,
                )
                .add_wgsl(
                    self.box_blur_h_shader,
                    struct.pack("iiiiii", radius, new_w, new_h, offset, border_mode, divisor_mode),
                    new_w,
                    new_h,
                )
            )
            src_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(
                self.expand_shader,
                struct.pack("iiiiffff", offset, offset, new_w, new_h, 0.0, 0.0, 0.0, 0.0),
                new_w,
                new_h,
            )

            current = current.add_parallel_wgsl([blur_branch, src_branch]).add_wgsl(
                self.composite_shader, struct.pack("f", strength), new_w, new_h
            )
            cur_w, cur_h = new_w, new_h

        current = current.add_wgsl(self.ycbcr_decode_shader, None, cur_w, cur_h)

        return GeneratorBuilderReturn(current, ItemResult(cur_w, cur_h))

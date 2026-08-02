import math
import struct

from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ..common.params import clamp, make_generator_information, pack_box_blur_dir_params, pack_expand_params
from ..common.shader_loader import compose_common_shader, effect_dirs, lib_module, shared_shader

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

        current_dir, common_dir = effect_dirs(__file__)
        color_module = lib_module(common_dir, "color")
        blur_module = lib_module(common_dir, "blur")

        self.box_blur_dir_shader = compose_common_shader("box_blur_dir", [blur_module], common_dir, "box_blur_dir.wgsl")
        self.expand_shader = shared_shader("expand", common_dir, "expand.wgsl")
        self.ycbcr_encode_shader = compose_common_shader("ycbcr_encode", [color_module], common_dir, "ycbcr_encode.wgsl")
        self.ycbcr_decode_shader = compose_common_shader("ycbcr_decode", [color_module], common_dir, "ycbcr_decode.wgsl")
        self.composite_shader = shared_shader("diffusion_light_composite", current_dir, "composite.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
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
        strength_ui = clamp(args.get("strength", 50), 0, 100)
        diffusion = clamp(args.get("diffusion", 12), 0, 500)
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
                    self.box_blur_dir_shader,
                    pack_box_blur_dir_params(radius, 0, 1, cur_w, new_h, offset, border_mode, divisor_mode),
                    cur_w,
                    new_h,
                )
                .add_wgsl(
                    self.box_blur_dir_shader,
                    pack_box_blur_dir_params(radius, 1, 0, new_w, new_h, offset, border_mode, divisor_mode),
                    new_w,
                    new_h,
                )
            )
            src_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(
                self.expand_shader,
                pack_expand_params(offset, offset, new_w, new_h),
                new_w,
                new_h,
            )

            current = current.add_parallel_wgsl([blur_branch, src_branch]).add_wgsl(
                self.composite_shader, struct.pack("f", strength), new_w, new_h
            )
            cur_w, cur_h = new_w, new_h

        current = current.add_wgsl(self.ycbcr_decode_shader, None, cur_w, cur_h)

        return GeneratorBuilderReturn(current, ItemResult(cur_w, cur_h))

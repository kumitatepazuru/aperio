import os
import struct

import aperio_plugin
from aperio import gpu_util
from aperio.gpu_util import PyCompiledWgsl
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ..common.border_correction import ab_constants
from ..common.color import bt601_encode
from ..common.params import clamp, make_generator_information, pack_box_average_dir_params
from ..common.shader_loader import compose_common_shader, effect_dirs, lib_module, shared_shader


class ColorKeyEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.color_key_effect"
        self.display_name = "カラーキー"
        self.description = "Keys out a chosen color using an axis-aligned YCbCr box, with alpha-only edge softening."

        current_dir, common_dir = effect_dirs(__file__)

        color_module = lib_module(common_dir, "color")
        blur_module = lib_module(common_dir, "blur")
        color_key_module = gpu_util.create_composable_module(os.path.join(current_dir, "common.wgsl"))

        self.flat_shader = PyCompiledWgsl.compose_new(
            "color_key_flat",
            [color_module, color_key_module],
            gpu_util.create_naga_module(os.path.join(current_dir, "color_key_flat.wgsl")),
            aperio_plugin.image_generator,
        )
        self.border_pass1_shader = PyCompiledWgsl.compose_new(
            "color_key_border_pass1",
            [color_module, color_key_module],
            gpu_util.create_naga_module(os.path.join(current_dir, "border_pass1.wgsl")),
            aperio_plugin.image_generator,
        )
        self.box_average_dir_shader = compose_common_shader(
            "color_key_box_average_dir", [blur_module], common_dir, "box_average_dir.wgsl"
        )
        self.border_pass3_shader = PyCompiledWgsl.compose_new(
            "color_key_border_pass3",
            [blur_module],
            gpu_util.create_naga_module(os.path.join(current_dir, "border_pass3.wgsl")),
            aperio_plugin.image_generator,
        )
        self.select_shader = shared_shader("color_key_select", common_dir, "select.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Color(
                    id="key_color",
                    title="キー色",
                    default_value=(0.0, 1.0, 0.0, 1.0),
                    use_alpha=False,
                ),
                RequestStructureParameter.Int(
                    id="luma_range",
                    title="輝度範囲",
                    default_value=0,
                    min=0,
                    max=4096,
                ),
                RequestStructureParameter.Int(
                    id="chroma_range",
                    title="色差範囲",
                    default_value=0,
                    min=0,
                    max=4096,
                ),
                RequestStructureParameter.Int(
                    id="border_correction",
                    title="境界補正",
                    default_value=0,
                    suffix="px",
                    min=0,
                    max=5,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        key_color = args.get("key_color", (0.0, 1.0, 0.0, 1.0))
        luma_range_ui = clamp(args.get("luma_range", 0), 0, 4096)
        chroma_range_ui = clamp(args.get("chroma_range", 0), 0, 4096)
        border_correction = clamp(args.get("border_correction", 0), 0, 5)

        w, h = params.width, params.height

        key_cb, key_cr, key_y = bt601_encode(key_color[0], key_color[1], key_color[2])
        # README §9: raw 0..4096 は bt601_encode の正規化空間(y: 0..1, cb/cr: -0.5..0.5)に
        # 対して /4096.0 で1:1対応する(+0x74 拡張ブロックが無く表示スケールが1のため)。
        luma_range = luma_range_ui / 4096.0
        chroma_range = chroma_range_ui / 4096.0

        key_params = struct.pack("fffff", key_y, key_cb, key_cr, luma_range, chroma_range)

        if border_correction == 0:
            builder = gpu_util.PyImageGenerateBuilder().add_wgsl(self.flat_shader, key_params, w, h)
            return GeneratorBuilderReturn(builder, ItemResult(w, h))

        r = border_correction
        a_const, b_const = ab_constants(r)

        pass1_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(self.border_pass1_shader, key_params, w, h)
        original_branch = gpu_util.PyImageGenerateBuilder()
        # state: [0]=a0マップ(パス1), [1]=元画像
        stage1 = gpu_util.PyImageGenerateBuilder().add_parallel_wgsl([pass1_branch, original_branch])

        v_params = pack_box_average_dir_params(r, 0, 1, w, h)
        v_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(self.box_average_dir_shader, v_params, w, h)
        keep_a0_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(self.select_shader, struct.pack("i", 0), w, h)
        keep_orig_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(self.select_shader, struct.pack("i", 1), w, h)
        # state: [0]=垂直方向にボックス平均済みのa0, [1]=a0そのまま(未ぼかし), [2]=元画像
        stage2 = stage1.add_parallel_wgsl([v_branch, keep_a0_branch, keep_orig_branch])

        pass3_params = struct.pack("iiiff", r, w, h, a_const, b_const)
        builder = stage2.add_wgsl(self.border_pass3_shader, pass3_params, w, h)

        return GeneratorBuilderReturn(builder, ItemResult(w, h))

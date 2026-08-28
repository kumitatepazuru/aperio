import math
import os
import struct

import aperio_plugin
from aperio import gpu_util
from aperio.gpu_util import PyCompiledWgsl
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.border_correction import ab_constant
from ...common.color import bt601_encode
from ...common.params import clamp, make_generator_information, pack_box_average_dir_params
from ...common.shader_loader import compose_common_shader, effect_dirs, lib_module, shared_shader


class ConvertGamutEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.convert_gamut_effect"
        self.display_name = "特定色域変換"
        self.description = "Recolors pixels close to a chosen hue/saturation toward a target color."

        current_dir, common_dir = effect_dirs(__file__)

        color_module = lib_module(common_dir, "color")
        blur_module = lib_module(common_dir, "blur")
        gamut_module = gpu_util.create_composable_module(os.path.join(current_dir, "common.wgsl"))

        self.flat_shader = PyCompiledWgsl.compose_new(
            "convert_gamut_flat",
            [color_module, gamut_module],
            gpu_util.create_naga_module(os.path.join(current_dir, "flat.wgsl")),
            aperio_plugin.image_generator,
        )
        self.border_pass1_shader = PyCompiledWgsl.compose_new(
            "convert_gamut_border_pass1",
            [color_module, gamut_module],
            gpu_util.create_naga_module(os.path.join(current_dir, "border_pass1.wgsl")),
            aperio_plugin.image_generator,
        )
        self.box_average_dir_shader = compose_common_shader(
            "convert_gamut_box_average_dir", [blur_module], common_dir, "box_average_dir.wgsl"
        )
        self.border_pass3_shader = PyCompiledWgsl.compose_new(
            "convert_gamut_border_pass3",
            [blur_module, color_module, gamut_module],
            gpu_util.create_naga_module(os.path.join(current_dir, "border_pass3.wgsl")),
            aperio_plugin.image_generator,
        )
        self.select_shader = shared_shader("convert_gamut_select", common_dir, "select.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Color(
                    id="before_color", title="変換前の色", default_value=(0.0, 1.0, 0.0, 1.0), use_alpha=False
                ),
                RequestStructureParameter.Color(
                    id="after_color", title="変換後の色", default_value=(0.0, 0.0, 1.0, 1.0), use_alpha=False
                ),
                RequestStructureParameter.Float(
                    id="hue_range", title="色相範囲", default_value=8.0, min=0.0, max=256.0
                ),
                RequestStructureParameter.Float(
                    id="saturation_range", title="彩度範囲", default_value=8.0, min=0.0, max=256.0
                ),
                RequestStructureParameter.Int(
                    id="border_correction", title="境界補正", default_value=2, suffix="px", min=0, max=8
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn:
        args = params.args
        before_color = args.get("before_color", (0.0, 1.0, 0.0, 1.0))
        after_color = args.get("after_color", (0.0, 0.0, 1.0, 1.0))
        hue_range_raw = max(0.0, min(256.0, float(args.get("hue_range", 8.0))))
        sat_range_raw = max(0.0, min(256.0, float(args.get("saturation_range", 8.0))))
        # border_correctionはボックス平均カーネル半径・境界補正伸張式(1/r)の分母に
        # 直結するため、下限0(ゼロ除算防止)は維持する。
        border_correction = clamp(int(args.get("border_correction", 2)), 0, 8)

        w, h = params.width, params.height

        key_cb, key_cr, key_y = bt601_encode(before_color[0], before_color[1], before_color[2])
        after_cb, after_cr, after_y = bt601_encode(after_color[0], after_color[1], after_color[2])
        key_hue = math.atan2(key_cr, key_cb)
        # exedit-inspect convert_gamut README §5: key_sat はゼロ除算防止のため下限を持つ
        # (原典の「key_sat<=0ならkey_sat=1」に相当)。
        key_sat = max(abs(key_cb), abs(key_cr), 1e-3)

        hue_range_turns = hue_range_raw / 512.0
        sat_range = (sat_range_raw / 256.0) * key_sat

        if border_correction == 0:
            flat_params = struct.pack(
                "ffffffff",
                key_hue, key_sat, key_y,
                after_cr, after_cb, after_y,
                hue_range_turns, sat_range,
            )
            builder = gpu_util.PyImageGenerateBuilder().add_wgsl(self.flat_shader, flat_params, w, h)
            return GeneratorBuilderReturn(builder, ItemResult(w, h))

        r = border_correction
        a_const = ab_constant(r)

        pass1_params = struct.pack("ffff", key_hue, key_sat, hue_range_turns, sat_range)
        pass1_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(self.border_pass1_shader, pass1_params, w, h)
        original_branch = gpu_util.PyImageGenerateBuilder()
        # state: [0]=距離dマップ(パス1), [1]=元画像
        stage1 = gpu_util.PyImageGenerateBuilder().add_parallel_wgsl([pass1_branch, original_branch])

        v_params = pack_box_average_dir_params(r, 0, 1, w, h)
        v_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(self.box_average_dir_shader, v_params, w, h)
        keep_dist_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(self.select_shader, struct.pack("i", 0), w, h)
        keep_orig_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(self.select_shader, struct.pack("i", 1), w, h)
        # state: [0]=垂直方向に平均済みの距離d, [1]=距離dそのまま(未ぼかし), [2]=元画像
        stage2 = stage1.add_parallel_wgsl([v_branch, keep_dist_branch, keep_orig_branch])

        pass3_params = struct.pack(
            "iiiffffff",
            r, w, h, a_const,
            key_sat, key_y,
            after_cr, after_cb, after_y,
        )
        builder = stage2.add_wgsl(self.border_pass3_shader, pass3_params, w, h)

        return GeneratorBuilderReturn(builder, ItemResult(w, h))

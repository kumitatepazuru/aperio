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


class ChromaKeyEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.chroma_key_effect"
        self.display_name = "クロマキー"
        self.description = "Keys out a chosen color by hue/saturation distance, with spill unmixing and edge erosion."

        current_dir, common_dir = effect_dirs(__file__)

        color_module = lib_module(common_dir, "color")
        blur_module = lib_module(common_dir, "blur")
        chroma_key_module = gpu_util.create_composable_module(os.path.join(current_dir, "common.wgsl"))

        self.flat_shader = PyCompiledWgsl.compose_new(
            "chroma_key_flat",
            [color_module, chroma_key_module],
            gpu_util.create_naga_module(os.path.join(current_dir, "chroma_key_flat.wgsl")),
            aperio_plugin.image_generator,
        )
        self.border_pass3_shader = PyCompiledWgsl.compose_new(
            "chroma_key_border_pass3",
            [color_module, blur_module, chroma_key_module],
            gpu_util.create_naga_module(os.path.join(current_dir, "border_pass3.wgsl")),
            aperio_plugin.image_generator,
        )

        # border_pass1: hue_excessが最大約8.0まで未クランプで出るが、符号なし・
        # 軽度の超過で下流も1〜2ホップのため16で足りる。
        rgba16float = gpu_util.WrappedImagePixelFormat.Rgba16Float
        self.border_pass1_shader = PyCompiledWgsl.compose_new(
            "chroma_key_border_pass1",
            [color_module, chroma_key_module],
            gpu_util.create_naga_module(os.path.join(current_dir, "border_pass1.wgsl")),
            aperio_plugin.image_generator,
            min_output_format=rgba16float,
        )
        # box_average_dir: 上記マップを平均化するだけで精度要求は上がらないため16。
        self.box_average_dir_shader = compose_common_shader(
            "chroma_key_box_average_dir", [blur_module], common_dir, "box_average_dir.wgsl",
            min_output_format=rgba16float,
        )
        # select: 上記マップの単純コピー持ち越しのため16。
        self.select_shader = shared_shader(
            "chroma_key_select", common_dir, "select.wgsl", min_output_format=rgba16float
        )

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
                    id="hue_range",
                    title="色相範囲",
                    default_value=24,
                    min=0,
                    max=256,
                ),
                RequestStructureParameter.Int(
                    id="sat_range",
                    title="彩度範囲",
                    default_value=96,
                    min=0,
                    max=256,
                ),
                RequestStructureParameter.Int(
                    id="border_correction",
                    title="境界補正",
                    default_value=1,
                    suffix="px",
                    min=0,
                    max=5,
                ),
                RequestStructureParameter.Bool(
                    id="color_correction",
                    title="色彩補正",
                    default_value=False,
                ),
                RequestStructureParameter.Bool(
                    id="alpha_correction",
                    title="透過補正",
                    default_value=False,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        key_color = args.get("key_color", (0.0, 1.0, 0.0, 1.0))
        hue_range_ui = args.get("hue_range", 24)
        sat_range_ui = args.get("sat_range", 96)
        # border_correctionはボックス平均カーネル半径・境界補正伸張式(1/r)の分母に
        # 直結するため、下限0(ゼロ除算防止)は維持する。
        border_correction = clamp(args.get("border_correction", 1), 0, 5)
        color_correction = bool(args.get("color_correction", False))
        # 透過補正は色彩補正offでは常に無効(exedit-inspect chroma_key README §5/§7)。
        alpha_correction = bool(args.get("alpha_correction", False)) and color_correction

        w, h = params.width, params.height

        key_cb, key_cr, _ = bt601_encode(key_color[0], key_color[1], key_color[2])
        key_sat = max(abs(key_cb), abs(key_cr))
        hue_range_turns = hue_range_ui / 512.0
        sat_range = (sat_range_ui / 256.0) * key_sat

        color_correction_flag = 1 if color_correction else 0
        alpha_correction_flag = 1 if alpha_correction else 0

        if border_correction == 0:
            flat_params = struct.pack(
                "fffffii", key_cb, key_cr, key_sat, hue_range_turns, sat_range, color_correction_flag, alpha_correction_flag
            )
            builder = gpu_util.PyImageGenerateBuilder().add_wgsl(self.flat_shader, flat_params, w, h)
            return GeneratorBuilderReturn(builder, ItemResult(w, h))

        r = border_correction
        a_const = ab_constant(r)

        pass1_params = struct.pack("fffff", key_cb, key_cr, key_sat, hue_range_turns, sat_range)
        pass1_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(self.border_pass1_shader, pass1_params, w, h)
        original_branch = gpu_util.PyImageGenerateBuilder()
        # state: [0]=map_b/map_c(パス1), [1]=元画像
        stage1 = gpu_util.PyImageGenerateBuilder().add_parallel_wgsl([pass1_branch, original_branch])

        v_params = pack_box_average_dir_params(r, 0, 1, w, h)
        v_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(self.box_average_dir_shader, v_params, w, h)
        keep_map_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(self.select_shader, struct.pack("i", 0), w, h)
        keep_orig_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(self.select_shader, struct.pack("i", 1), w, h)
        # state: [0]=垂直方向にボックス平均済みのmap, [1]=パス1そのまま(未ぼかし), [2]=元画像
        stage2 = stage1.add_parallel_wgsl([v_branch, keep_map_branch, keep_orig_branch])

        pass3_params = struct.pack(
            "iiiffffii", r, w, h, a_const, key_cb, key_cr, key_sat, color_correction_flag, alpha_correction_flag
        )
        builder = stage2.add_wgsl(self.border_pass3_shader, pass3_params, w, h)

        return GeneratorBuilderReturn(builder, ItemResult(w, h))

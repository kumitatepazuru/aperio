import os
import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio.gpu_util import PyCompiledWgsl
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters


def _bt601_encode(r: float, g: float, b: float) -> tuple[float, float, float]:
    """common/lib/color.wgsl の bt601_encode と同じ式(キー色は1画素分なのでPython側で
    一度だけ計算し、全画素で使い回す)。戻り値は (cb, cr, y)。"""
    y = 0.299 * r + 0.587 * g + 0.114 * b
    cr = (r - y) / 1.402
    cb = (b - y) / 1.772
    return cb, cr, y


class ChromaKeyEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.chroma_key_effect"
        self.display_name = "クロマキー"
        self.description = "Keys out a chosen color by hue/saturation distance, with spill unmixing and edge erosion."

        current_dir = os.path.dirname(__file__)
        common_dir = os.path.join(current_dir, "..", "common")

        def load(path: str) -> str:
            with open(path, "r") as f:
                return f.read()

        color_module = gpu_util.create_composable_module(os.path.join(common_dir, "lib", "color.wgsl"))
        blur_module = gpu_util.create_composable_module(os.path.join(common_dir, "lib", "blur.wgsl"))
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

        self.border_pass1_shader = PyCompiledWgsl.compose_new(
            "chroma_key_border_pass1",
            [color_module, chroma_key_module],
            gpu_util.create_naga_module(os.path.join(current_dir, "border_pass1.wgsl")),
            aperio_plugin.image_generator,
        )
        self.box_average_dir_shader = PyCompiledWgsl.compose_new(
            "chroma_key_box_average_dir",
            [blur_module],
            gpu_util.create_naga_module(os.path.join(common_dir, "box_average_dir.wgsl")),
            aperio_plugin.image_generator,
        )
        self.select_shader = PyCompiledWgsl(
            "chroma_key_select", load(os.path.join(common_dir, "select.wgsl")), aperio_plugin.image_generator, None
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
        hue_range_ui = max(0, min(256, args.get("hue_range", 24)))
        sat_range_ui = max(0, min(256, args.get("sat_range", 96)))
        border_correction = max(0, min(5, args.get("border_correction", 1)))
        color_correction = bool(args.get("color_correction", False))
        # 透過補正は色彩補正offでは常に無効(exedit-inspect chroma_key README §5/§7)。
        alpha_correction = bool(args.get("alpha_correction", False)) and color_correction

        w, h = params.width, params.height

        key_cb, key_cr, _ = _bt601_encode(key_color[0], key_color[1], key_color[2])
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
        # README §7: A = 4096 - 4096/r, B = 4096 - (4096/r)*r(いずれも整数除算)。
        # r は 1〜5 の整数なので Python の floor division でそのまま丸め誤差なく再現できる。
        a_int = 4096 - 4096 // r
        b_int = 4096 - (4096 // r) * r
        a_const = a_int / 4096.0
        b_const = b_int / 4096.0

        pass1_params = struct.pack("fffff", key_cb, key_cr, key_sat, hue_range_turns, sat_range)
        pass1_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(self.border_pass1_shader, pass1_params, w, h)
        original_branch = gpu_util.PyImageGenerateBuilder()
        # state: [0]=map_b/map_c(パス1), [1]=元画像
        stage1 = gpu_util.PyImageGenerateBuilder().add_parallel_wgsl([pass1_branch, original_branch])

        v_params = struct.pack("iiiii", r, 0, 1, w, h)
        v_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(self.box_average_dir_shader, v_params, w, h)
        keep_map_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(self.select_shader, struct.pack("i", 0), w, h)
        keep_orig_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(self.select_shader, struct.pack("i", 1), w, h)
        # state: [0]=垂直方向にボックス平均済みのmap, [1]=パス1そのまま(未ぼかし), [2]=元画像
        stage2 = stage1.add_parallel_wgsl([v_branch, keep_map_branch, keep_orig_branch])

        pass3_params = struct.pack(
            "iiifffffii", r, w, h, a_const, b_const, key_cb, key_cr, key_sat, color_correction_flag, alpha_correction_flag
        )
        builder = stage2.add_wgsl(self.border_pass3_shader, pass3_params, w, h)

        return GeneratorBuilderReturn(builder, ItemResult(w, h))

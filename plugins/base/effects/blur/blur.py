import struct

from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ..common.params import clamp, make_generator_information, pack_box_blur_dir_params
from ..common.shader_loader import compose_common_shader, effect_dirs, lib_module


def _split_radius(radius: int) -> tuple[int, int]:
    """半径を2つのボックスパスに分割する。合成カーネルが三角形(半径が奇数なら
    上底3の台形)になる、実機ぼかしフィルタの分割方法(README手順2・3)。
    r_hi = ceil(r/2), r_lo = floor(r/2)。"""
    return radius - radius // 2, radius // 2


class BlurEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.blur_effect"
        self.display_name = "ぼかし"
        self.description = "Applies a blur effect to the input frame."

        current_dir, common_dir = effect_dirs(__file__)
        color_module = lib_module(common_dir, "color")
        math_module = lib_module(common_dir, "math")
        blur_module = lib_module(common_dir, "blur")

        self.box_blur_dir_shader = compose_common_shader(
            "box_blur_dir", [blur_module], common_dir, "box_blur_dir.wgsl",
            output_format=gpu_util.WrappedImagePixelFormat.Rgba32Float,
        )
        self.curve_shader = compose_common_shader(
            "curve", [math_module], common_dir, "curve.wgsl",
            output_format=gpu_util.WrappedImagePixelFormat.Rgba32Float,
        )
        self.ycbcr_encode_shader = compose_common_shader("ycbcr_encode", [color_module], common_dir, "ycbcr_encode.wgsl")
        self.ycbcr_decode_shader = compose_common_shader("ycbcr_decode", [color_module], common_dir, "ycbcr_decode.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
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
        aspect = clamp(args.get("aspect", 0), -100, 100)
        light_intensity = clamp(args.get("light_intensity", 0), 0, 60)
        fixed_size = bool(args.get("fixed_size", False))

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

        width, height = params.width, params.height

        # サイズ固定オフ(キャンバスが伸びる)は常にカーネル幅で割ってフェード
        # させ(divisor_mode=0)、サイズ固定オンは有効サンプル数で再正規化する
        # (divisor_mode=1)。境界は常にゼロパディング(border_mode=1、実機挙動)。
        divisor_mode = 1 if fixed_size else 0
        border_mode = 1

        builder = gpu_util.PyImageGenerateBuilder()

        use_curve = light_intensity > 0
        curve_base = 1.0
        if use_curve:
            curve_base = 1.0 + max(1, min(100, light_intensity)) * 0.001
            builder = builder.add_wgsl(self.ycbcr_encode_shader, None, width, height)
            builder = builder.add_wgsl(self.curve_shader, struct.pack("fii", curve_base, 0, 2), width, height)

        cur_w, cur_h = width, height

        def h_pass(b: gpu_util.PyImageGenerateBuilder, radius: int) -> gpu_util.PyImageGenerateBuilder:
            nonlocal cur_w
            if radius <= 0:
                return b
            offset = 0 if fixed_size else radius
            new_w = cur_w if fixed_size else cur_w + 2 * radius
            shader_params = pack_box_blur_dir_params(radius, 1, 0, new_w, cur_h, offset, border_mode, divisor_mode)
            b = b.add_wgsl(self.box_blur_dir_shader, shader_params, new_w, cur_h)
            cur_w = new_w
            return b

        def v_pass(b: gpu_util.PyImageGenerateBuilder, radius: int) -> gpu_util.PyImageGenerateBuilder:
            nonlocal cur_h
            if radius <= 0:
                return b
            offset = 0 if fixed_size else radius
            new_h = cur_h if fixed_size else cur_h + 2 * radius
            shader_params = pack_box_blur_dir_params(radius, 0, 1, cur_w, new_h, offset, border_mode, divisor_mode)
            b = b.add_wgsl(self.box_blur_dir_shader, shader_params, cur_w, new_h)
            cur_h = new_h
            return b

        rx_hi, rx_lo = _split_radius(h_radius)
        ry_hi, ry_lo = _split_radius(v_radius)

        builder = h_pass(builder, rx_hi)
        builder = h_pass(builder, rx_lo)
        builder = v_pass(builder, ry_hi)
        builder = v_pass(builder, ry_lo)

        if use_curve:
            builder = builder.add_wgsl(self.curve_shader, struct.pack("fii", curve_base, 1, 2), cur_w, cur_h)
            builder = builder.add_wgsl(self.ycbcr_decode_shader, None, cur_w, cur_h)

        return GeneratorBuilderReturn(builder, ItemResult(cur_w, cur_h))

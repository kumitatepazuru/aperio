import struct

from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import clamp, make_generator_information, pack_box_blur_dir_params, split_radius
from ...common.shader_loader import compose_common_shader, effect_dirs, lib_module


class SharpEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.sharp_effect"
        self.display_name = "シャープ"
        self.description = "Unsharp-masks the object: blurs a copy with a box blur, then adds back the difference from the original."

        current_dir, common_dir = effect_dirs(__file__)
        blur_module = lib_module(common_dir, "blur")
        color_module = lib_module(common_dir, "color")

        # box_blur_dir: `ぼかし`の`サイズ固定`オン版と命令単位で同一(README §4)。
        # blur.pyと同じ名前・フォーマットフロアでコンパイルしてパイプラインキャッシュを
        # 共有する。light_intensity経由の用途と共有しているため32必須。
        self.box_blur_dir_shader = compose_common_shader(
            "box_blur_dir", [blur_module], common_dir, "box_blur_dir.wgsl",
            min_output_format=gpu_util.WrappedImagePixelFormat.Rgba32Float,
        )
        # sharp_composite: strengthの上限(表示800.0 = ×8)と1/pの増幅(最大4096倍、
        # README §5.2)が重なると値が大きく張り出しうるため32必須。
        self.composite_shader = compose_common_shader(
            "sharp_composite", [color_module], current_dir, "sharp_composite.wgsl",
            min_output_format=gpu_util.WrappedImagePixelFormat.Rgba32Float,
        )

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Float(
                    id="strength",
                    title="強さ",
                    default_value=50.0,
                    min=0.0,
                    max=800.0,
                ),
                RequestStructureParameter.Int(
                    id="range",
                    title="範囲",
                    default_value=5,
                    suffix="px",
                    min=0,
                    max=100,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        strength_ui = args.get("strength", 50.0)
        # rangeはブラーカーネル半径(split_radius経由)に直結するため、下限0は維持する。
        range_ = clamp(args.get("range", 5), 0, 100)

        # README §2: 強さ<=0または範囲==0は独立した2本の早期returnで、画像に一切触れない。
        if strength_ui <= 0 or range_ == 0:
            return None

        strength = strength_ui / 100.0

        w, h = params.width, params.height

        r_hi, r_lo = split_radius(range_)

        def blur_pass(b: gpu_util.PyImageGenerateBuilder, radius: int, step_x: int, step_y: int) -> gpu_util.PyImageGenerateBuilder:
            if radius <= 0:
                return b
            shader_params = pack_box_blur_dir_params(radius, step_x, step_y, w, h, divisor_mode=1)
            return b.add_wgsl(self.box_blur_dir_shader, shader_params, w, h)

        # 元画像の退避(README「スクラッチ」に相当) ―― 何もしないブランチがそのまま
        # 上流の状態(素の入力)を素通しする。
        original_branch = gpu_util.PyImageGenerateBuilder()

        # README §4: V(r_hi) H(r_hi) V(r_lo) H(r_lo)。`サイズ固定`オンと同じく
        # offset=0(キャンバス不変)・border_mode=1(範囲外は寄与ゼロ、いずれもデフォルト)
        # ・divisor_mode=1(有効サンプル数で正規化)。
        blur_branch = gpu_util.PyImageGenerateBuilder()
        blur_branch = blur_pass(blur_branch, r_hi, 0, 1)
        blur_branch = blur_pass(blur_branch, r_hi, 1, 0)
        blur_branch = blur_pass(blur_branch, r_lo, 0, 1)
        blur_branch = blur_pass(blur_branch, r_lo, 1, 0)

        composite_params = struct.pack("f", strength)
        builder = gpu_util.PyImageGenerateBuilder().add_parallel_wgsl([original_branch, blur_branch]).add_wgsl(
            self.composite_shader, composite_params, w, h
        )

        return GeneratorBuilderReturn(builder, ItemResult(w, h))

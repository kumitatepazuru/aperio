import struct

from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ..common.params import clamp, make_generator_information, pack_box_blur_dir_params
from ..common.shader_loader import compose_common_shader, effect_dirs, lib_module, shared_shader


def _axis_radii(radius: int, aspect: int) -> tuple[int, int]:
    """範囲・縦横比 → (rx, ry)。縦横比は片方の軸だけを縮める
    (README: 縦横比>0でry、縦横比<0でrxのみ変化。もう片方はradiusのまま)。"""
    if aspect >= 0:
        rx = radius
        ry = round(radius * (1 - aspect / 100))
    else:
        rx = round(radius * (1 + aspect / 100))
        ry = radius
    return rx, ry


class BoundaryBlurEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.boundary_blur_effect"
        self.display_name = "境界ぼかし"
        self.description = "Erodes or blurs the alpha channel near the edges of the input frame."

        current_dir, common_dir = effect_dirs(__file__)
        blur_module = lib_module(common_dir, "blur")

        self.shader = shared_shader("boundary_blur", current_dir, "boundary_blur.wgsl")
        self.box_blur_dir_shader = compose_common_shader("box_blur_dir", [blur_module], common_dir, "box_blur_dir.wgsl")
        self.box_blur_h_alpha_merge_shader = compose_common_shader(
            "boundary_blur_box_blur_h_alpha_merge", [blur_module], current_dir, "box_blur_h_alpha_merge.wgsl"
        )

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Int(
                    id="radius",
                    title="範囲",
                    default_value=30,
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
                RequestStructureParameter.Bool(
                    id="alpha_boundary",
                    title="透明度の境界をぼかす",
                    default_value=False,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        radius = max(0, args.get("radius", 30))
        aspect = clamp(args.get("aspect", 0), -100, 100)
        alpha_boundary = bool(args.get("alpha_boundary", False))

        if radius <= 0:
            return None

        width, height = params.width, params.height

        if not alpha_boundary:
            packed = struct.pack("ii", radius, aspect)
            builder = gpu_util.PyImageGenerateBuilder().add_wgsl(self.shader, packed, width, height)
            return GeneratorBuilderReturn(builder, ItemResult(width, height))

        # 透明度境界モード(README 4): 元のアルファを2パスのボックスぼかし
        # (ゼロ埋め境界・常にフルカーネル幅で正規化)で実際にぼかし、元のアルファ
        # との非線形なしきい値カーブで再合成する。キャンバス拡張は無い。
        rx, ry = _axis_radii(radius, aspect)
        rx = min(rx, width // 2)
        ry = min(ry, height // 2)

        box_blur_v_branch = gpu_util.PyImageGenerateBuilder()
        if ry > 0:
            v_params = pack_box_blur_dir_params(ry, 0, 1, width, height)
            box_blur_v_branch = box_blur_v_branch.add_wgsl(self.box_blur_dir_shader, v_params, width, height)

        # box_blur_h + alpha_merge を1シェーダーに統合(box_blur_h_alpha_merge.wgsl)。
        # radius=0の水平パスは単一タップ=恒等になるため、rx=0でも特別扱い不要。
        h_params = struct.pack("iiiiii", rx, width, height, 0, 1, 0)
        builder = (
            gpu_util.PyImageGenerateBuilder()
            .add_parallel_wgsl([box_blur_v_branch, gpu_util.PyImageGenerateBuilder()])
            .add_wgsl(self.box_blur_h_alpha_merge_shader, h_params, width, height)
        )

        return GeneratorBuilderReturn(builder, ItemResult(width, height))

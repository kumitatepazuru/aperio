import os
import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio.gpu_util import PyCompiledWgsl
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters


def _axis_radii(radius: int, aspect: int) -> tuple[int, int]:
    """範囲・縦横比 → (rx, ry)。縦横比は片方の軸だけを縮める
    (README: 縦横比>0でry、縦横比<0でrxのみ変化。もう片方はradiusのまま)。"""
    if aspect >= 0:
        rx = radius
        ry = int(radius * (1 - aspect / 100))
    else:
        rx = int(radius * (1 + aspect / 100))
        ry = radius
    return rx, ry


class BoundaryBlurEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.boundary_blur_effect"
        self.display_name = "境界ぼかし"
        self.description = "Erodes or blurs the alpha channel near the edges of the input frame."

        current_dir = os.path.dirname(__file__)
        common_dir = os.path.join(current_dir, "..", "common")
        blur_module = gpu_util.create_composable_module(os.path.join(common_dir, "lib", "blur.wgsl"))

        def load(path: str) -> str:
            with open(path, "r") as f:
                return f.read()

        self.shader = PyCompiledWgsl(
            "boundary_blur", load(os.path.join(current_dir, "boundary_blur.wgsl")), aperio_plugin.image_generator, None
        )
        self.box_blur_dir_shader = PyCompiledWgsl.compose_new(
            "box_blur_dir",
            [blur_module],
            gpu_util.create_naga_module(os.path.join(common_dir, "box_blur_dir.wgsl")),
            aperio_plugin.image_generator,
        )
        self.box_blur_h_alpha_merge_shader = PyCompiledWgsl.compose_new(
            "boundary_blur_box_blur_h_alpha_merge",
            [blur_module],
            gpu_util.create_naga_module(os.path.join(current_dir, "box_blur_h_alpha_merge.wgsl")),
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
        aspect = max(-100, min(100, args.get("aspect", 0)))
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
            v_params = struct.pack("iiiiiiii", ry, 0, 1, width, height, 0, 1, 0)
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

import os
import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio.gpu_util import PyCompiledWgsl
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters


def _jfa_step_sizes(max_dim: int) -> list[int]:
    """JFA(ジャンプフラッディング法)で使うstep幅の列を、大きい方から1まで並べて返す。
    JFAは近似アルゴリズムであるため、複雑な形状の境界付近では誤って少し違う種を
    選んでしまい、それがブロック状のノイズとして残ることがある。
    最後にstep=1のパスを2回追加する(JFA+2)ことで、この取りこぼしの大半を修正する。"""
    step = 1
    while step * 2 < max_dim:
        step *= 2
    steps = []
    while step >= 1:
        steps.append(step)
        step //= 2
    steps.append(1)
    steps.append(1)
    return steps


def _pointer_jump_pass_count(width: int, height: int) -> int:
    """島の中心(距離場の極大点)まで山登り経路をポインタジャンプで辿り切るのに
    十分なパス数。経路長は最長でも画像の対角線分程度に収まるため、
    log2(width + height) 回も繰り返せば実用上十分収束する。"""
    max_path = max(width + height, 1)
    passes = 1
    count = 0
    while passes < max_path:
        passes *= 2
        count += 1
    return max(count, 1)


class BoundaryBlurEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.boundary_blur_effect"
        self.display_name = "境界ぼかし"
        self.description = "Fades the edges of the input frame using a sine-based alpha curve."

        current_dir = os.path.dirname(__file__)

        def load(filename: str) -> str:
            with open(os.path.join(current_dir, filename), "r") as f:
                return f.read()

        self.shader = PyCompiledWgsl("boundary_blur", load("boundary_blur.wgsl"), aperio_plugin.image_generator, None)
        self.seed_init_shader = PyCompiledWgsl(
            "boundary_blur_alpha_seed_init", load("alpha_seed_init.wgsl"), aperio_plugin.image_generator, None
        )
        self.jfa_step_shader = PyCompiledWgsl(
            "boundary_blur_alpha_jfa_step", load("alpha_jfa_step.wgsl"), aperio_plugin.image_generator, None
        )
        self.peak_init_shader = PyCompiledWgsl(
            "boundary_blur_alpha_peak_init", load("alpha_peak_init.wgsl"), aperio_plugin.image_generator, None
        )
        self.pointer_jump_shader = PyCompiledWgsl(
            "boundary_blur_alpha_pointer_jump", load("alpha_pointer_jump.wgsl"), aperio_plugin.image_generator, None
        )
        self.alpha_merge_shader = PyCompiledWgsl(
            "boundary_blur_alpha_merge", load("alpha_merge.wgsl"), aperio_plugin.image_generator, None
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
            builder = (
                gpu_util.PyImageGenerateBuilder()
                .add_wgsl(self.shader, packed, width, height)
            )
            return GeneratorBuilderReturn(builder, ItemResult(width, height))

        # 透明度境界モード:
        # 1. JFAで各ピクセルから最も近い透明ピクセル(=不透明領域の境界)までの座標を求める
        # 2. そこから求めた境界距離Dが3x3近傍で最大となる方向へポインタを繋ぎ、
        #    ポインタジャンプで連結した不透明領域(島)ごとの中心(最も奥まった点)へ収束させる
        # 3. 各ピクセルは自身の島の中心から見た方向に応じて楕円状にフェードする
        computed = gpu_util.PyImageGenerateBuilder().add_wgsl(self.seed_init_shader, None, width, height)

        for step in _jfa_step_sizes(max(width, height)):
            step_params = struct.pack("i", step)
            computed = computed.add_wgsl(self.jfa_step_shader, step_params, width, height)

        computed = computed.add_wgsl(self.peak_init_shader, None, width, height)

        for _ in range(_pointer_jump_pass_count(width, height)):
            computed = computed.add_wgsl(self.pointer_jump_shader, None, width, height)

        merge_params = struct.pack("ii", radius, aspect)
        builder = (
            gpu_util.PyImageGenerateBuilder()
            .add_parallel_wgsl([gpu_util.PyImageGenerateBuilder(), computed])
            .add_wgsl(self.alpha_merge_shader, merge_params, width, height)
        )

        return GeneratorBuilderReturn(builder, ItemResult(width, height))

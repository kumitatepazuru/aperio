import struct

from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import (
    GeneratorBuilderReturn,
    GeneratorWgslReturn,
    VideoEffectGeneratorBase,
    VideoGenerateParameters,
)

from ...common.params import make_generator_information
from ...common.shader_loader import compose_common_shader, effect_dirs, lib_module, shared_shader

# 作り直す行の偶奇。`奇数解除` は奇数行を作り直すので1、`偶数解除` は0
# (exedit-inspect deinterlacing README §4: 名前は「捨てるほう」を指している)。
_FIELD_PARITY = {"odd": 1, "even": 0}


class DeinterlacingEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.deinterlacing_effect"
        self.display_name = "インターレース解除"
        self.description = "Removes interlace combing with a spatial edge-adaptive interpolation or a 1:2:1 vertical filter."

        current_dir, common_dir = effect_dirs(__file__)
        color_module = lib_module(common_dir, "color")

        # deinterlace_field: 判定が成分ごとに独立しているためYとCr/Cbが別々の近傍を選ぶことが
        # あり、RGBへ戻すと[0,1]を最大+0.44はみ出す(上下=純青・斜め4つ=白でB=1.44)。
        # 1パスで完結し桁落ちも指数カーブも無いので16で足りる。
        self.field_shader = compose_common_shader(
            "deinterlace_field", [color_module], current_dir, "deinterlace_field.wgsl",
            min_output_format=gpu_util.WrappedImagePixelFormat.Rgba16Float,
        )
        # deinterlace_blend: 1:2:1は凸結合なので値域が入力を超えない。フロア不要。
        self.blend_shader = shared_shader("deinterlace_blend", current_dir, "deinterlace_blend.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.List(
                    id="mode",
                    title="解除方法",
                    values={"odd": "奇数解除", "even": "偶数解除", "double": "二重化"},
                    default_value="odd",
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn | GeneratorBuilderReturn | None:
        mode = params.args.get("mode", "odd")
        w, h = params.width, params.height

        if mode == "double":
            # README §7: `worker_blend` のガードは h < 2。
            if h < 2:
                return None
            # パラメータを持たないシェーダーなのでbuilder経由で渡す(common/ycbcr_encode.wgsl
            # などと同じ形)。
            builder = gpu_util.PyImageGenerateBuilder().add_wgsl(self.blend_shader, None, w, h)
            return GeneratorBuilderReturn(builder, ItemResult(w, h))

        # README §7: `worker_field` は ceil(h/2) < 2、つまり h <= 2 で何もせずに帰る。
        if h <= 2:
            return None

        # README §4: 未知の選択値は `奇数解除` と同じ扱いになる。
        field_parity = _FIELD_PARITY.get(mode, 1)
        return GeneratorWgslReturn(self.field_shader, struct.pack("i", field_parity), ItemResult(w, h))

import struct

from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.shader_loader import compose_common_shader, effect_dirs, lib_module


class MozaicEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.mozaic_effect"
        self.display_name = "モザイク"
        self.description = "Applies a mosaic effect to the input frame, aligned to the center."

        current_dir, common_dir = effect_dirs(__file__)
        color_module = lib_module(common_dir, "color")
        math_module = lib_module(common_dir, "math")

        self.mozaic_h_shader = compose_common_shader("mozaic_h", [math_module], current_dir, "mozaic_h.wgsl")
        # エフェクト最終段の単発。ベベルの輝度/色差補正は固定±25%のみで超過は僅かなため16で足りる。
        self.mozaic_v_shader = compose_common_shader(
            "mozaic_v", [math_module, color_module], current_dir, "mozaic_v.wgsl",
            min_output_format=gpu_util.WrappedImagePixelFormat.Rgba16Float,
        )

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Int(
                    id="size",
                    title="サイズ",
                    default_value=12,
                    suffix="px",
                    min=1,
                ),
                RequestStructureParameter.Bool(
                    id="tile",
                    title="タイル風",
                    default_value=False,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        size = max(1, args.get("size", 12))
        tile = bool(args.get("tile", False))

        # サイズ2未満は完全な無処理(実機は画像に触れずにTRUEを返す)。
        if size < 2:
            return None

        width, height = params.width, params.height

        # 中心を基準にブロックを区切るため、割り切れない端数は上下左右の端で切り取られる
        center_x = width // 2
        center_y = height // 2

        h_params = struct.pack("iiii", size, center_x, width, height)
        v_params = struct.pack("iiiiii", size, center_x, center_y, width, height, 1 if tile else 0)

        builder = (
            gpu_util.PyImageGenerateBuilder()
            .add_wgsl(self.mozaic_h_shader, h_params, width, height)
            .add_wgsl(self.mozaic_v_shader, v_params, width, height)
        )

        return GeneratorBuilderReturn(builder, ItemResult(width, height))

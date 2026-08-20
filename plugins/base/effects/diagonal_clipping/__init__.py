import math
import struct

from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.shader_loader import effect_dirs, shared_shader


class DiagonalClippingEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.diagonal_clipping_effect"
        self.display_name = "斜めクリッピング"
        self.description = "Clips the object's alpha along a line at any angle, with a soft gradient edge and half-plane/keep-band/remove-band modes."

        current_dir, _ = effect_dirs(__file__)
        self.shader = shared_shader("diagonal_clipping", current_dir, "diagonal_clipping.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Int(
                    id="center_x",
                    title="中心X",
                    default_value=0,
                    suffix="px",
                    min=-2000,
                    max=2000,
                ),
                RequestStructureParameter.Int(
                    id="center_y",
                    title="中心Y",
                    default_value=0,
                    suffix="px",
                    min=-2000,
                    max=2000,
                ),
                RequestStructureParameter.Float(
                    id="angle",
                    title="角度",
                    default_value=0.0,
                    suffix="°",
                    min=-3600.0,
                    max=3600.0,
                ),
                RequestStructureParameter.Int(
                    id="blur",
                    title="ぼかし",
                    default_value=1,
                    suffix="px",
                    min=0,
                    max=2000,
                ),
                RequestStructureParameter.Int(
                    id="width",
                    title="幅",
                    default_value=0,
                    suffix="px",
                    min=-2000,
                    max=2000,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn:
        args = params.args
        center_x = args.get("center_x", 0)
        center_y = args.get("center_y", 0)
        angle = args.get("angle", 0.0)
        # bandの除数になるため下限0は維持する(wipe.pyのblurクランプと同じ理由)。
        blur = max(0, args.get("blur", 1))
        width = args.get("width", 0)

        # README §4: 単位ベクトルのまま扱う(Q16固定小数点への変換は原作バイナリの
        # 整数演算を再現するためだけの精度欠落なので廃止、convex_edge.pyと同じ方針)。
        # 凸エッジの(-sinθ,+cosθ)とはちょうど逆向き。
        theta = math.radians(angle)
        nx = math.sin(theta)
        ny = -math.cos(theta)

        w, h = params.width, params.height
        # 中心は原作と同じ整数除算(w/2 * 0.5にすると偶数サイズで境界が半画素ずれる)。
        cx = float(w // 2 + center_x)
        cy = float(h // 2 + center_y)
        band = float(blur + 1)

        shader_params = struct.pack("ffffff", cx, cy, nx, ny, band, float(width))
        return GeneratorWgslReturn(self.shader, shader_params, ItemResult(w, h))

import math
import struct

from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.shader_loader import compose_common_shader, effect_dirs, lib_module, shared_shader


class ConvexEdgeEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.convex_edge_effect"
        self.display_name = "凸エッジ"
        self.description = "Adds a directional alpha-derivative bevel/emboss to the object's luminance without touching alpha or growing the canvas."

        current_dir, common_dir = effect_dirs(__file__)

        color_module = lib_module(common_dir, "color")

        self.convex_edge_shader = compose_common_shader(
            "convex_edge",
            [color_module],
            current_dir,
            "convex_edge.wgsl",
            min_output_format=gpu_util.WrappedImagePixelFormat.Rgba16Float,
        )
        self.identity_shader = shared_shader("convex_edge_select", common_dir, "select.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Int(
                    id="width",
                    title="幅",
                    default_value=4,
                    suffix="px",
                    min=0,
                    max=100,
                ),
                RequestStructureParameter.Float(
                    id="height",
                    title="高さ",
                    default_value=1.0,
                    min=0.0,
                    max=3.0,
                ),
                RequestStructureParameter.Float(
                    id="angle",
                    title="角度",
                    default_value=-45.0,
                    suffix="°",
                    min=-360.0,
                    max=360.0,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn:
        args = params.args
        width = int(args.get("width", 4))
        height = args.get("height", 1.0)
        angle = args.get("angle", -45.0)

        w, h = params.width, params.height

        # README §2: 幅はオブジェクトの寸法(縦横それぞれの半分)でもクランプされる。
        # これがwidth自体の唯一必要なクランプで、負値・過大値もここで自然に処理される。
        steps = int(min(width, w // 2, h // 2))

        if steps <= 0:
            # README §2: 幅==0、またはクランプ後に幅<=0(=オブジェクトが2px未満)は
            # バッファを入れ替えない早期リターン。恒等コピーで代用する。
            return GeneratorWgslReturn(self.identity_shader, struct.pack("i", 0), ItemResult(w, h))

        # README §3: t = radians(角度)*-1。方向ベクトルは単位ベクトルのままシェーダーへ渡す
        # (Q16固定小数点への変換は原作バイナリの整数演算を再現するためだけの精度欠落なので廃止)。
        t = math.radians(angle) * -1.0
        dir_x = math.sin(t)
        dir_y = math.cos(t)

        shader_params = struct.pack("ifff", steps, dir_x, dir_y, height)
        return GeneratorWgslReturn(self.convex_edge_shader, shader_params, ItemResult(w, h))

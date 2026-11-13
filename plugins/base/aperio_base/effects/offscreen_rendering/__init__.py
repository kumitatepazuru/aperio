from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation
from aperio_plugin.event_manager import event
from aperio_plugin.frame_builder import layer_params
from aperio_plugin.plugin_base.generator_base import GeneratorShaderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.shader_loader import effect_dirs, shared_slang_shader


class OffscreenRenderingEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.offscreen_rendering_effect"
        self.display_name = "オフスクリーン描画"
        self.description = (
            "Draws the object as it appears on screen into a frame-sized transparent buffer "
            "and replaces the object image with it."
        )

        current_dir, _ = effect_dirs(__file__)
        # compose と同じサンプリング (clamp_to_edge / linear) で、最終描画と同じ見た目に焼き込む。
        # 出力は [0,1] のストレートアルファで1パスなので min_output_format は指定しない。
        self.offscreen_shader = shared_slang_shader(
            "offscreen_rendering",
            current_dir,
            "offscreen_rendering.slang",
            sampler_options=gpu_util.PySamplerOptions("clamp_to_edge", "linear"),
        )

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        # パラメータは持たない。挙動はここまでに積まれた変形(ItemResult)だけで決まる。
        return make_generator_information(self.display_name, [])

    def generate(self, params: VideoGenerateParameters) -> GeneratorShaderReturn | None:
        item_result = params.item_result
        # オフスクリーン済みなら何もしない
        if item_result.offscreen:
            return None

        # 座標・回転・拡大率・透明度・中心ずれ(+3D回転の透視)は最終合成と同じ式で求める。
        shader_params = layer_params(item_result, params.canvas_height)

        # 焼き込んだ後は変形を持たないキャンバス大の結果になり、焼き込み前の形は offscreen_source に残る。
        return GeneratorShaderReturn(
            self.offscreen_shader,
            shader_params,
            item_result.to_offscreen(params.canvas_width, params.canvas_height),
        )

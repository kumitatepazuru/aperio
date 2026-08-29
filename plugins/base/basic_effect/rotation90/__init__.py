import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.shader_loader import effect_dirs, shared_shader


class Rotation90Effect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "basic_effect.rotation90"
        self.display_name = "ローテーション"
        self.description = "Rotates the canvas in 90-degree increments."

        current_dir, _ = effect_dirs(__file__)
        self.shader = shared_shader("base_effect_rotation90", current_dir, "rotation90.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Int(
                    id="rot90",
                    title="90度回転",
                    default_value=0,
                    min=-4,
                    max=4,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn | GeneratorBuilderReturn:
        raw = int(params.args.get("rot90", 0))
        if raw == 0 or raw == 4 or raw == -4:
            return GeneratorBuilderReturn(gpu_util.PyImageGenerateBuilder(), ItemResult(params.width, params.height))

        mode = raw % 4

        # 180度(mode==2)は幅高さを入れ替えない
        swap = mode % 2 != 0
        new_width = params.height if swap else params.width
        new_height = params.width if swap else params.height
        if swap:
            # 入れ替えた新しい軸を wgpu上の確保済み最大キャンバスへクランプする
            # 実機の処理に相当する安全策(入力自体が上限内なので通常は発火しない)。
            max_dim = aperio_plugin.image_generator.maximum_texture_size
            new_width = min(new_width, max_dim)
            new_height = min(new_height, max_dim)

        shader_params = struct.pack("iii", mode, new_width, new_height)

        return GeneratorWgslReturn(
            self.shader,
            shader_params,
            ItemResult(new_width, new_height),
        )

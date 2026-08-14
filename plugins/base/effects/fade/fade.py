import struct

import aperio_plugin
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ..common.params import make_generator_information
from ..common.shader_loader import effect_dirs, shared_shader


class FadeEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.fade_effect"
        self.display_name = "フェード"
        self.description = "Fades the object's alpha in/out near the start/end of its duration."

        current_dir, _ = effect_dirs(__file__)
        self.fade_shader = shared_shader("fade", current_dir, "fade.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Float(
                    id="fade_in",
                    title="イン",
                    default_value=0.5,
                    suffix="s",
                    min=0.0,
                    max=10.0,
                ),
                RequestStructureParameter.Float(
                    id="fade_out",
                    title="アウト",
                    default_value=0.5,
                    suffix="s",
                    min=0.0,
                    max=10.0,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn | None:
        args = params.args
        fade_in = args.get("fade_in", 0.5)
        fade_out = args.get("fade_out", 0.5)

        fps = aperio_plugin.store_manager.get_state().frame_state.fps
        in_frames = round(fade_in * fps)
        out_frames = round(fade_out * fps)

        # アイテム内のフレーム位置(README §3)。item.end は排他境界なので
        # end-1 が原作の inclusive な終端フレームに相当する。
        t = params.frame_number - params.layer.start
        u = (params.layer.end - 1) - params.frame_number

        # README §4: フェードイン側の `if (a < 4096)` は到達不能なガードなので
        # 省略し、min()1本にまとめている。
        g = 1.0
        if t < in_frames:
            g = min(g, (t + 1) / (in_frames + 1))
        if u < out_frames:
            g = min(g, (u + 1) / (out_frames + 1))

        if g >= 1.0:
            return None

        shader_params = struct.pack("f", g)
        return GeneratorWgslReturn(self.fade_shader, shader_params, ItemResult(params.width, params.height))

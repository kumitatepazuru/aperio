import os
import struct

import aperio_plugin
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio.gpu_util import PyCompiledWgsl
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters


class ClipEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.clip_effect"
        self.display_name = "クリッピング"
        self.description = "Clips the input frame from specified directions."

        current_dir = os.path.dirname(__file__)
        with open(os.path.join(current_dir, "clip.wgsl"), "r") as f:
            self.clip_shader = PyCompiledWgsl("clip", f.read(), aperio_plugin.image_generator, None)

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
                    id="clip_top",
                    title="上",
                    default_value=0,
                    suffix="px",
                    min=0,
                ),
                RequestStructureParameter.Int(
                    id="clip_bottom",
                    title="下",
                    default_value=0,
                    suffix="px",
                    min=0,
                ),
                RequestStructureParameter.Int(
                    id="clip_left",
                    title="左",
                    default_value=0,
                    suffix="px",
                    min=0,
                ),
                RequestStructureParameter.Int(
                    id="clip_right",
                    title="右",
                    default_value=0,
                    suffix="px",
                    min=0,
                ),
                RequestStructureParameter.Bool(
                    id="move_center",
                    title="中心の位置を変更",
                    default_value=False,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn:
        args = params.args
        clip_top = max(0, args.get("clip_top", 0))
        clip_bottom = max(0, args.get("clip_bottom", 0))
        clip_left = max(0, args.get("clip_left", 0))
        clip_right = max(0, args.get("clip_right", 0))
        move_center: bool = args.get("move_center", False)

        new_width = max(1, params.width - clip_left - clip_right)
        new_height = max(1, params.height - clip_top - clip_bottom)

        # シェーダーに渡すオフセットは入力サイズを超えないようにクランプ
        shader_clip_left = min(clip_left, params.width - 1)
        shader_clip_top = min(clip_top, params.height - 1)

        shader_params = struct.pack("iiii", shader_clip_left, shader_clip_top, new_width, new_height)

        if move_center:
            return GeneratorWgslReturn(self.clip_shader, shader_params, ItemResult(new_width, new_height))

        # center_x/center_y でクリップ前の位置を維持する
        # クランプ後の実効 shader_clip_left/top と new_width/height から逆算することで、
        # クリップ量が width/height を超えた場合でもズレが生じない。
        center_x = params.width // 2 - (shader_clip_left + new_width // 2)
        center_y = params.height // 2 - (shader_clip_top + new_height // 2)
        return GeneratorWgslReturn(self.clip_shader, shader_params, ItemResult(new_width, new_height, center_x=center_x, center_y=center_y))

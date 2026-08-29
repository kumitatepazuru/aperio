import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.shader_loader import effect_dirs, shared_shader


class ExpandEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "basic_effect.expand"
        self.display_name = "領域拡張"
        self.description = "Extends the canvas in specified directions."

        current_dir, _ = effect_dirs(__file__)
        self.expand_shader = shared_shader("base_effect_expand", current_dir, "expand.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Int(
                    id="top",
                    title="上",
                    default_value=0,
                    suffix="px",
                    min=0,
                ),
                RequestStructureParameter.Int(
                    id="bottom",
                    title="下",
                    default_value=0,
                    suffix="px",
                    min=0,
                ),
                RequestStructureParameter.Int(
                    id="left",
                    title="左",
                    default_value=0,
                    suffix="px",
                    min=0,
                ),
                RequestStructureParameter.Int(
                    id="right",
                    title="右",
                    default_value=0,
                    suffix="px",
                    min=0,
                ),
                RequestStructureParameter.Bool(
                    id="fill",
                    title="塗りつぶし",
                    default_value=False,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn | GeneratorBuilderReturn:
        args = params.args
        top = max(0, int(args.get("top", 0)))
        bottom = max(0, int(args.get("bottom", 0)))
        left = max(0, int(args.get("left", 0)))
        right = max(0, int(args.get("right", 0)))
        fill = bool(args.get("fill", False))

        # wgpu上の確保済み最大キャンバスへのクランプ。テクスチャエラーによるpanic防止。
        max_dim = aperio_plugin.image_generator.maximum_texture_size
        room_w = max(0, max_dim - params.width)
        left = min(left, room_w)
        right = min(right, room_w - left)
        room_h = max(0, max_dim - params.height)
        top = min(top, room_h)
        bottom = min(bottom, room_h - top)

        if top == 0 and bottom == 0 and left == 0 and right == 0:
            return GeneratorBuilderReturn(gpu_util.PyImageGenerateBuilder(), ItemResult(params.width, params.height))

        new_width = params.width + left + right
        new_height = params.height + top + bottom

        center_x = (left - right) / 2
        center_y = (top - bottom) / 2

        shader_params = struct.pack("iiiii", left, top, new_width, new_height, 1 if fill else 0)

        return GeneratorWgslReturn(
            self.expand_shader,
            shader_params,
            ItemResult(new_width, new_height, center_x=center_x, center_y=center_y),
        )

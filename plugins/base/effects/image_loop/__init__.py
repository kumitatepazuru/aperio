import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.shader_loader import compose_common_shader, effect_dirs, lib_module, shared_shader


class ImageLoopEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.image_loop_effect"
        self.display_name = "画像ループ"
        self.description = "Tiles the object and scrolls the tiled result."

        current_dir, common_dir = effect_dirs(__file__)
        math_module = lib_module(common_dir, "math")

        self.resize_shader = compose_common_shader(
            "resize_bilinear", [math_module], common_dir, "resize_bilinear.wgsl",
        )
        self.tile_shader = shared_shader("image_loop", current_dir, "image_loop.wgsl")

    # TODO: 原作の「個別オブジェクト」チェックボックスは未実装。
    # AviUtl側は複数オブジェクトへの分身生成だが、Aperioは「1エフェクト→1画像」のモデルのため対応する概念が存在しない。
    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Int(
                    id="tiles_x", title="横回数", default_value=1, min=1, max=400
                ),
                RequestStructureParameter.Int(
                    id="tiles_y", title="縦回数", default_value=1, min=1, max=400
                ),
                RequestStructureParameter.Float(
                    id="speed_x", title="速度X", default_value=0.0, suffix="px/frame", min=-1000.0, max=1000.0
                ),
                RequestStructureParameter.Float(
                    id="speed_y", title="速度Y", default_value=0.0, suffix="px/frame", min=-1000.0, max=1000.0
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        w, h = params.width, params.height
        tiles_x = max(1, int(args.get("tiles_x", 1)))
        tiles_y = max(1, int(args.get("tiles_y", 1)))
        speed_x = float(args.get("speed_x", 0.0))
        speed_y = float(args.get("speed_y", 0.0))
        if tiles_x == 1 and tiles_y == 1 and speed_x == 0.0 and speed_y == 0.0:
            return None

        max_dim = aperio_plugin.image_generator.maximum_texture_size
        fit_w = max_dim / tiles_x
        fit_h = max_dim / tiles_y
        scale = min(1.0, fit_w / w, fit_h / h)
        tile_w = max(1, round(w * scale))
        tile_h = max(1, round(h * scale))
        ow, oh = tile_w * tiles_x, tile_h * tiles_y

        builder = gpu_util.PyImageGenerateBuilder()
        if scale < 1.0:
            builder = builder.add_wgsl(self.resize_shader, struct.pack("ii", tile_w, tile_h), tile_w, tile_h)

        elapsed_frames = params.frame_number - params.layer.start
        offset_x = round(speed_x * scale * elapsed_frames) % tile_w
        offset_y = round(speed_y * scale * elapsed_frames) % tile_h

        builder = builder.add_wgsl(
            self.tile_shader,
            struct.pack("iiiiii", tile_w, tile_h, offset_x, offset_y, ow, oh),
            ow, oh,
        )
        return GeneratorBuilderReturn(builder, ItemResult(ow, oh))

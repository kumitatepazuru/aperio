from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information


class ZoomEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "basic_effect.zoom"
        self.display_name = "拡大率"
        self.description = "Scales the object size by specified ratios."

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Float(
                    id="zoom",
                    title="拡大率",
                    default_value=100.0,
                    suffix="％",
                    min=0.0,
                ),
                RequestStructureParameter.Float(
                    id="zoom_x",
                    title="X",
                    default_value=100.0,
                    suffix="％",
                    min=0.0,
                ),
                RequestStructureParameter.Float(
                    id="zoom_y",
                    title="Y",
                    default_value=100.0,
                    suffix="％",
                    min=0.0,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn:
        args = params.args
        zoom = float(args.get("zoom", 100.0)) / 100.0
        zoom_x = float(args.get("zoom_x", 100.0)) / 100.0
        zoom_y = float(args.get("zoom_y", 100.0)) / 100.0

        scale_x = zoom * zoom_x
        scale_y = zoom * zoom_y

        return GeneratorBuilderReturn(
            gpu_util.PyImageGenerateBuilder(),
            ItemResult(params.width, params.height, scale=(scale_x, scale_y)),
        )

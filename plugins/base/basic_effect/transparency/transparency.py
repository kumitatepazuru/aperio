from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information


class TransparencyEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "basic_effect.transparency"
        self.display_name = "透明度"
        self.description = "Adjusts object transparency."

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Float(
                    id="transparency",
                    title="透明度",
                    default_value=0.0,
                    suffix="％",
                    min=0.0,
                    max=100.0,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn:
        args = params.args
        transparency = float(args.get("transparency", 0.0))
        alpha = max(0.0, min(1.0, 1.0 - transparency / 100.0))

        return GeneratorBuilderReturn(
            gpu_util.PyImageGenerateBuilder(),
            ItemResult(params.width, params.height, alpha=alpha),
        )

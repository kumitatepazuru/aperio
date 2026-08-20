from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information


class PositionEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "basic_effect.position"
        self.display_name = "座標"
        self.description = "Offsets object position in 3D coordinates."

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Float(
                    id="x",
                    title="X",
                    default_value=0.0,
                    suffix="px",
                ),
                RequestStructureParameter.Float(
                    id="y",
                    title="Y",
                    default_value=0.0,
                    suffix="px",
                ),
                RequestStructureParameter.Float(
                    id="z",
                    title="Z",
                    default_value=0.0,
                    suffix="px",
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn:
        args = params.args
        x = float(args.get("x", 0.0))
        y = float(args.get("y", 0.0))
        z = float(args.get("z", 0.0))

        return GeneratorBuilderReturn(
            gpu_util.PyImageGenerateBuilder(),
            ItemResult(params.width, params.height, pos=(x, y, z)),
        )

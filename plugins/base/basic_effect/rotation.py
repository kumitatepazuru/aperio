from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ..common.params import make_generator_information


class RotationEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "basic_effect.rotation"
        self.display_name = "回転"
        self.description = "Rotates the object around 3D axes."

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Float(
                    id="rotate_x",
                    title="X",
                    default_value=0.0,
                    suffix="°",
                ),
                RequestStructureParameter.Float(
                    id="rotate_y",
                    title="Y",
                    default_value=0.0,
                    suffix="°",
                ),
                RequestStructureParameter.Float(
                    id="rotate_z",
                    title="Z",
                    default_value=0.0,
                    suffix="°",
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn:
        args = params.args
        rx = float(args.get("rotate_x", 0.0))
        ry = float(args.get("rotate_y", 0.0))
        rz = float(args.get("rotate_z", 0.0))

        return GeneratorBuilderReturn(
            gpu_util.PyImageGenerateBuilder(),
            ItemResult(params.width, params.height, rotate=(rx, ry, rz)),
        )

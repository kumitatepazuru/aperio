from aperio.item_structures import GeneratorEvent, GeneratorInformation, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorTextureReturn, VideoGenerateParameters, VideoObjectGeneratorBase


class GroupObject(VideoObjectGeneratorBase):

    def __init__(self):
        super().__init__()

        self.name = "base_object.group_object"
        self.display_name = "グループ"
        self.description = "グループオブジェクト。複数のレイヤーをまとめて扱うことができる。"

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return GeneratorInformation(
            display_name=self.display_name,
            duration_frames=300,
            max_frame=None,
            min_frame=None,
            structure=[
                RequestStructureParameter.Int("layers", "対象レイヤー数", 1, min=1),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorTextureReturn | None:
        return None

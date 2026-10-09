import struct

from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorShaderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.shader_loader import effect_dirs, shared_slang_shader


class FlipEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "basic_effect.flip"
        self.display_name = "反転"
        self.description = "Inverts spatial axes, luma, chroma, or alpha of the image."

        current_dir, _ = effect_dirs(__file__)
        self.shader = shared_slang_shader("base_effect_flip", current_dir, "flip.slang")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Bool(
                    id="flip_v",
                    title="上下反転",
                    default_value=False,
                ),
                RequestStructureParameter.Bool(
                    id="flip_h",
                    title="左右反転",
                    default_value=False,
                ),
                RequestStructureParameter.Bool(
                    id="invert_luma",
                    title="輝度反転",
                    default_value=False,
                ),
                RequestStructureParameter.Bool(
                    id="invert_chroma",
                    title="色相反転",
                    default_value=False,
                ),
                RequestStructureParameter.Bool(
                    id="invert_alpha",
                    title="透明度反転",
                    default_value=False,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorShaderReturn | None:
        args = params.args
        fv = 1 if args.get("flip_v", False) else 0
        fh = 1 if args.get("flip_h", False) else 0
        il = 1 if args.get("invert_luma", False) else 0
        ic = 1 if args.get("invert_chroma", False) else 0
        ia = 1 if args.get("invert_alpha", False) else 0

        if not (fv or fh or il or ic or ia):
            return None

        shader_params = struct.pack("iiiiiiii", fv, fh, il, ic, ia, 0, 0, 0)

        return GeneratorShaderReturn(self.shader, shader_params, ItemResult(params.width, params.height))

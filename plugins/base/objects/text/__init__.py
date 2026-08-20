import aperio_plugin
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio.gpu_util import PyCompiledTextureFunc
from aperio.text_rendering import PyTextRenderer, PyTextSpec
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorTextureReturn, VideoGenerateParameters, VideoObjectGeneratorBase


class TextObject(VideoObjectGeneratorBase):
    """
    テキストをレンダリングするオブジェクトプラグイン。テキストの内容、色、フォント、字間を指定してフレームにテキストを描画することができる。
    """

    def __init__(self):
        super().__init__()

        self.name = "base_object.text_object"
        self.display_name = "テキスト"
        self.description = "テキストを表示できます。テキストの内容、色、フォント、字間を指定できます。"

        text_renderer: PyTextRenderer = aperio_plugin.text_renderer
        self.compiled_func = PyCompiledTextureFunc("text_render", text_renderer.render_text_for_pipeline)
        self.text_renderer = text_renderer

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return GeneratorInformation(
            display_name=self.display_name,
            duration_frames=300,
            max_frame=None,
            min_frame=None,
            structure=[
                RequestStructureParameter.Textarea("text", "文字", "ここにテキストを入力"),
                RequestStructureParameter.Color("color", "色", (1.0, 1.0, 1.0, 1.0), use_alpha=True),
                RequestStructureParameter.Font("font", "フォント"),
                RequestStructureParameter.Int("font_size", "フォントサイズ", 48, suffix="px", min=1),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorTextureReturn | None:
        args = params.args

        text = args.get("text", "ここにテキストを入力")
        font = args.get("font", {})
        prepared = self.text_renderer.prepare_render_text(
            PyTextSpec(
                text=text,
                font_size=args.get("font_size", 48),
                color=args.get("color", (1.0, 1.0, 1.0, 1.0)),
                font_family=font.get("family", None),
                font_weight=font.get("weight", 400),
                max_width=None,
                line_spacing=1.0,
                char_spacing=0.0,
            )
        )

        if prepared is None:
            return None

        return GeneratorTextureReturn(
            compiled=self.compiled_func,
            params=prepared,
            item_result=ItemResult(prepared.width, prepared.height),
        )

from aperio.frame_structure import RequestStructureParameter
from aperio.text_rendering import PyTextRenderer, PyTextSpec
from aperio.gpu_util import PyCompiledTextureFunc, PyImageGenerator

from aperio_plugin.plugin_base.generator_base import GeneratorTextureReturn, NewObjectGeneratorReturn, ObjectGeneratorBase


class TextObject(ObjectGeneratorBase):
    """
    テキストをレンダリングするオブジェクトプラグイン。テキストの内容、色、フォント、字間を指定してフレームにテキストを描画することができる。
    """

    def __init__(self, generator: PyImageGenerator):
        super().__init__(generator)

        self.name = "base.text_object"
        self.display_name = "テキスト"
        self.description = "テキストを表示できます。テキストの内容、色、フォント、字間を指定できます。"

        self.base_structure: list[RequestStructureParameter] = [
              RequestStructureParameter.String("text", "テキスト内容", "Hello, Aperio!"),
        ]
        self.text_renderer = PyTextRenderer(generator)
        self.compiled_func = PyCompiledTextureFunc("text_render", self.text_renderer.render_text_for_pipeline)


    def on_new(self, args: dict) -> NewObjectGeneratorReturn:
        return NewObjectGeneratorReturn(display_name=self.display_name, duration_frames=300, structure=self.base_structure)
    
    def on_request_structure(self, params: dict) -> list[RequestStructureParameter]:
        return self.base_structure
        

    def generate(self, frame_number: int, args: dict, width: int, height: int) -> GeneratorTextureReturn | None:
        text = args.get("text", "Hello, Aperio!")
        prepared = self.text_renderer.prepare_render_text(
            PyTextSpec(
                text=text,
                font_size=48,
                color=(255, 255, 255, 255),
                font_family=None,
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
            output_width=prepared.width,
            output_height=prepared.height,
        )
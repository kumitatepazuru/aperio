import os

from aperio.avloader import PyVideoLoader
from aperio.frame_structure import FileFilter, RequestStructureParameter
from aperio.gpu_util import PyCompiledTextureFunc, PyImageGenerator

from aperio_plugin.plugin_base.generator_base import GenerateParameters, GeneratorTextureReturn, NewObjectGeneratorReturn, ObjectGeneratorBase


class VideoObject(ObjectGeneratorBase):
    """
    動画を再生するオブジェクトプラグイン。動画ファイルのパスを指定してフレームに動画を再生することができる。
    """

    def __init__(self, image_generator: PyImageGenerator):
        super().__init__()

        self.name = "base.video_object"
        self.display_name = "動画"
        self.description = "動画を再生できます。動画ファイルのパスを指定してください。"

        self.base_structure: list[RequestStructureParameter] = [
            RequestStructureParameter.File("video_path", "ファイル", False, "file", 
                                           [FileFilter("動画ファイル", ["mp4", "mkv", "webm", "avi", "flv", "mpeg", "mpg", "mov", "wmv", "mts", "m2ts", "m4v"])],
                                           ),
        ]
        self.image_generator = image_generator
        self.compiled_func: PyCompiledTextureFunc | None = None
        self.video_loader = None
        self.previous_video_path = ""

    def on_new(self, args: dict) -> NewObjectGeneratorReturn:
        return NewObjectGeneratorReturn(display_name=self.display_name, duration_frames=300, structure=self.base_structure)
    
    def on_request_structure(self, params: dict) -> list[RequestStructureParameter]:
        return self.base_structure
        

    def generate(self, params: GenerateParameters) -> GeneratorTextureReturn | None:
        path = params.args.get("video_path", "")[0]
        if path != self.previous_video_path and os.path.exists(path):
            self.video_loader = PyVideoLoader(path=path, target_fps=params.fps, image_generator=self.image_generator)
            self.compiled_func = PyCompiledTextureFunc("video_frame", self.video_loader.get_frame_for_pipeline)
            self.previous_video_path = path
        if self.video_loader is None or self.compiled_func is None:
            return None

        # 取得する動画のフレーム番号を計算する。
        video_frame_number = params.frame_number - params.layer.get("from", 0) + 1 # 動画のフレーム番号は1始まり

        return GeneratorTextureReturn(
            compiled=self.compiled_func,
            params=video_frame_number,
            output_width=self.video_loader.width,
            output_height=self.video_loader.height,
        )
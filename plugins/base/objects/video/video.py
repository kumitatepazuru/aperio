import os

import aperio_plugin
from aperio.avloader import PyVideoLoader
from aperio.frame_structure import FileFilter, GeneratorEvent, NewGeneratorReturn, RequestStructureParameter
from aperio.gpu_util import PyCompiledTextureFunc
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GenerateParameters, GeneratorTextureReturn, ObjectGeneratorBase


class VideoObject(ObjectGeneratorBase):
    """
    動画を再生するオブジェクトプラグイン。動画ファイルのパスを指定してフレームに動画を再生することができる。
    """

    def __init__(self):
        super().__init__()

        self.name = "base.video_object"
        self.display_name = "動画"
        self.description = "動画を再生できます。動画ファイルのパスを指定してください。"

        self.base_structure: list[RequestStructureParameter] = [
            RequestStructureParameter.File(
                "video_path",
                "ファイル",
                False,
                "file",
                [
                    FileFilter(
                        "動画ファイル",
                        ["mp4", "mkv", "webm", "avi", "flv", "mpeg", "mpg", "mov", "wmv", "mts", "m2ts", "m4v"],
                    )
                ],
            ),
        ]
        self.compiled_funcs: dict[str, PyCompiledTextureFunc] = {}
        self.video_loaders: dict[str, PyVideoLoader] = {}

    @event(type=GeneratorEvent.New)
    def on_new(self, params: dict) -> NewGeneratorReturn:
        return NewGeneratorReturn(
            display_name=self.display_name,
            duration_frames=300,
            structure=self.base_structure,
        )

    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, params: dict) -> list[RequestStructureParameter]:
        return self.base_structure

    def generate(self, params: GenerateParameters) -> GeneratorTextureReturn | None:
        paths = params.args.get("video_path", [""])
        if not paths:
            return None

        path = paths[0]
        if path not in self.video_loaders:
            # TODO: 開けなかったときにUIで通知する
            if os.path.exists(path):
                loader = PyVideoLoader(path=path, target_fps=params.fps, image_generator=aperio_plugin.image_generator)
                self.video_loaders[path] = loader
                self.compiled_funcs[path] = PyCompiledTextureFunc("video_frame", loader.get_frame_for_pipeline)

        video_loader = self.video_loaders.get(path)
        compiled_func = self.compiled_funcs.get(path)
        if video_loader is None or compiled_func is None:
            return None

        video_frame_number = params.frame_number - params.layer.get("from", 0) + 1

        return GeneratorTextureReturn(
            compiled=compiled_func,
            params=video_frame_number,
            output_width=video_loader.width,
            output_height=video_loader.height,
        )

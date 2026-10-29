import math
import os

import aperio_plugin
from aperio.avloader import PyVideoLoader
from aperio.item_structures import FileFilter, GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorTextureReturn, VideoGenerateParameters, VideoObjectGeneratorBase

from .. import write_video_sync_frame
from ..common.video_cache import VideoLoaderCache


class VideoObject(VideoObjectGeneratorBase):
    """
    動画を再生するオブジェクトプラグイン。動画ファイルのパスを指定してフレームに動画を再生することができる。
    """

    def __init__(self):
        super().__init__()

        self.name = "base_object.video_object"
        self.display_name = "動画"
        self.description = "動画を再生できます。動画ファイルのパスを指定してください。"

        # generate() は毎フレーム呼ばれ、オブジェクトの id (structure_id) を
        # キーとして使えるので、composite_video effect と共通の id キー・キャッシュを使う。
        self.video_cache = VideoLoaderCache("video_frame")

        # on_request_structure には id が渡ってこない(New/RequestStructure イベント時点では
        # タイムライン上の id が params に含まれない)ため、max_frame 計算に必要な
        # frame_count/fps を読むためだけの、パスキーの単純なローダー保持。
        # video_cache (id キー、generate 用) とは別物として扱う。
        self._metadata_loaders: dict[str, PyVideoLoader] = {}

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, params: dict) -> GeneratorInformation:
        paths = params.get("video_path", [])
        path = paths[0] if paths else ""
        if path not in self._metadata_loaders:
            # TODO: 開けなかったときにUIで通知する
            if os.path.exists(path):
                self._metadata_loaders[path] = PyVideoLoader(path=path, image_generator=aperio_plugin.image_generator)

        fps = aperio_plugin.store_manager.get_state().frame_state.fps
        return GeneratorInformation(
            display_name=self.display_name,
            duration_frames=300,
            max_frame=math.ceil(loader.frame_count * fps / loader.fps) if (loader := self._metadata_loaders.get(path)) else None,
            min_frame=None,
            structure=[
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
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorTextureReturn | None:
        paths = params.args.get("video_path", [""])
        if not paths:
            return None
        path = paths[0]
        entry = self.video_cache.get_or_load(params.structure_id, path) if path else None
        if entry is None:
            return None
        video_loader, compiled_func = entry

        video_frame_number = params.frame_number - params.layer.start + 1
        write_video_sync_frame(params.frame_number, video_frame_number)

        return GeneratorTextureReturn(
            compiled=compiled_func,
            params=(video_frame_number, aperio_plugin.store_manager.get_state().frame_state.fps),
            item_result=ItemResult(video_loader.width, video_loader.height),
        )

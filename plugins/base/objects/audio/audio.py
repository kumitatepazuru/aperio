import math
import os

from aperio import logger
import aperio_plugin
import numpy as np
import numpy.typing as npt

from aperio.avloader import PyAudioLoader
from aperio.item_structures import *
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import AudioGenerateParameters, AudioGeneratorReturn, AudioObjectGeneratorBase


class AudioObject(AudioObjectGeneratorBase):
    """音声ファイルを再生するオブジェクトプラグイン。"""

    def __init__(self):
        super().__init__()
        self.name = "base.audio_object"
        self.display_name = "音声"
        self.description = "音声ファイルを再生できます。音声ファイルのパスを指定してください。"
        self.audio_loaders: dict[str, PyAudioLoader] = {}

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, params: dict) -> GeneratorInformation:
        link_video = params.get("link_video", False)
        structure: list[RequestStructureParameter] = [RequestStructureParameter.Bool("link_video", "動画オブジェクトと連携", False)]
        max_frame = None

        if not link_video:
            paths = params.get("audio_path", [])
            path = paths[0] if paths else ""
            if path and path not in self.audio_loaders:
                if os.path.exists(path):
                    self.audio_loaders[path] = PyAudioLoader(path=path)

            fps = aperio_plugin.store_manager.get_state().frame_state.fps
            loader = self.audio_loaders.get(path)
            max_frame = math.ceil(loader.duration * fps) if loader else None
            structure.append(
                RequestStructureParameter.File(
                    "audio_path",
                    "ファイル",
                    False,
                    "file",
                    [
                        FileFilter(
                            "動画・音声ファイル",
                            ["mp4", "mkv", "webm", "avi", "flv", "mpeg", "mpg", "mov", "wmv", "mts", "m2ts", "m4v",
                             "mp3", "wav", "ogg", "flac", "aac", "m4a", "wma", "opus"],
                        )
                    ],
                )
            )

        return GeneratorInformation(
            display_name=self.display_name,
            duration_frames=300,
            max_frame=max_frame,
            min_frame=None,
            structure=structure,
        )

    def _find_linked_video(self, my_audio_item: ItemStructure.Audio, state):
        my_layer = my_audio_item.layer
        my_id = my_audio_item.id

        candidates = sorted(
            [item for item in state.timeline_items
             if isinstance(item, ItemStructure.Video)
             and item.layer < my_layer
             and item.object.get("name") == "base.video_object"
             and item.start < my_audio_item.end
             and item.end > my_audio_item.start],
            key=lambda x: x.layer,
            reverse=True,
        )

        for candidate in candidates:
            claimed = False
            for other in state.timeline_items:
                if not isinstance(other, ItemStructure.Audio):
                    continue
                if other.id == my_id:
                    continue
                if not other.object.get("parameters", {}).get("link_video", False):
                    continue
                other_candidates = sorted(
                    [item for item in state.timeline_items
                     if isinstance(item, ItemStructure.Video)
                     and item.layer < other.layer
                     and item.object.get("name") == "base.video_object"
                     and item.start < other.end
                     and item.end > other.start],
                    key=lambda x: x.layer,
                    reverse=True,
                )
                if other_candidates and other_candidates[0].id == candidate.id:
                    claimed = True
                    break
            if not claimed:
                return candidate

        return None

    def generate(self, params: AudioGenerateParameters) -> AudioGeneratorReturn | None:
        link_video = params.args.get("link_video", False)

        if link_video:
            state = aperio_plugin.store_manager.get_state()
            video_item = self._find_linked_video(params.layer, state)
            if video_item is None:
                return None

            video_paths = video_item.object.get("parameters", {}).get("video_path", [])
            if not video_paths:
                return None
            path = video_paths[0]

            if path and path not in self.audio_loaders:
                if os.path.exists(path):
                    self.audio_loaders[path] = PyAudioLoader(path=path)

            loader = self.audio_loaders.get(path)
            if loader is None:
                return None

            fps = state.frame_state.fps
            audio_time = params.start_time - video_item.start / fps
            if audio_time < 0 or audio_time >= loader.duration:
                return None
        else:
            paths = params.args.get("audio_path", [""])
            if not paths:
                return None
            path = paths[0]
            loader = self.audio_loaders.get(path)
            if loader is None:
                return None

            fps = aperio_plugin.store_manager.get_state().frame_state.fps
            audio_time = params.start_time - params.layer.start / fps
            if audio_time >= loader.duration:
                logger.error(f"audio_time >= duration, returning None")
                return None

        time_samples = max(0, round(audio_time * params.sample_rate))
        duration_samples = params.sample_count
        raw: npt.NDArray[np.float32] = loader.get_audio(
            time_samples,
            duration_samples,
            sample_rate=params.sample_rate,
            channels=params.channels,
        )

        if raw.shape[1] < duration_samples:
            pad = np.zeros((params.channels, duration_samples - raw.shape[1]), dtype=np.float32)
            raw = np.concatenate([raw, pad], axis=1)
        elif raw.shape[1] > duration_samples:
            raw = raw[:, : duration_samples]

        return AudioGeneratorReturn(samples=raw)

import math
import os

from aperio import logger
import aperio_plugin
import numpy as np
import numpy.typing as npt

from aperio.avloader import PyAudioLoader
from aperio.item_structures import FileFilter, GeneratorEvent, GeneratorInformation, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import AudioGenerateParameters, AudioObjectGeneratorBase


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
        paths = params.get("audio_path", [])
        path = paths[0] if paths else ""
        if path and path not in self.audio_loaders:
            if os.path.exists(path):
                self.audio_loaders[path] = PyAudioLoader(path=path)

        fps = aperio_plugin.store_manager.get_state().frame_state.fps
        loader = self.audio_loaders.get(path)
        return GeneratorInformation(
            display_name=self.display_name,
            duration_frames=300,
            max_frame=math.ceil(loader.duration * fps) if loader else None,
            min_frame=None,
            structure=[
                RequestStructureParameter.File(
                    "audio_path",
                    "ファイル",
                    False,
                    "file",
                    [
                        FileFilter(
                            "音声ファイル",
                            ["mp3", "wav", "ogg", "flac", "aac", "m4a", "wma", "opus"],
                        )
                    ],
                ),
            ],
        )

    def generate(self, params: AudioGenerateParameters) -> npt.NDArray[np.float32] | None:
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

        return raw

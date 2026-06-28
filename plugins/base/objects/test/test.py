from aperio import logger
import numpy as np
import numpy.typing as npt

from aperio.item_structures import GeneratorEvent, GeneratorInformation, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import AudioGenerateParameters, AudioObjectGeneratorBase


class SinWaveObject(AudioObjectGeneratorBase):
    """指定したHzのサイン波を生成するテスト用オーディオオブジェクトプラグイン。"""

    def __init__(self):
        super().__init__()
        self.name = "base.test.sin_wave"
        self.display_name = "サイン波"
        self.description = "指定したHzのサイン波を生成します。"

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return GeneratorInformation(
            display_name=self.display_name,
            duration_frames=300,
            max_frame=None,
            min_frame=None,
            structure=[
                RequestStructureParameter.Float("hz", "基準音（ド）の周波数", 261.63, "Hz"),
            ],
        )

    def generate(self, params: AudioGenerateParameters) -> npt.NDArray[np.float32] | None:
        base_hz = float(params.args.get("hz", 261.63))
        interval = 1  # 音階が変わる間隔（秒）

        t = params.start_time + np.arange(params.sample_count, dtype=np.float64) / params.sample_rate

        note_index = np.floor(t / interval).astype(np.int64)
        semitone = note_index % 12
        hz = base_hz * 2.0 ** (semitone / 12.0)

        # 各音の開始時点までの累積位相を計算してノート境界での位相連続性を保つ
        semitone_hz = base_hz * 2.0 ** (np.arange(12, dtype=np.float64) / 12.0)
        cum_phase_octave = np.cumsum(semitone_hz) * interval
        total_octave = cum_phase_octave[-1]

        octaves_completed = note_index // 12
        safe_index = np.maximum(semitone - 1, 0)
        phase_at_note_start = (
            octaves_completed * total_octave
            + np.where(semitone > 0, cum_phase_octave[safe_index], 0.0)
        )

        t_within = t - note_index * interval
        phase = 2.0 * np.pi * (phase_at_note_start + hz * t_within)
        wave = np.sin(phase).astype(np.float32)
        return np.tile(wave, (params.channels, 1))

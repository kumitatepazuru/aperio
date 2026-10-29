import math

import numpy as np
import numpy.typing as npt

# スピーカーアジマス角 (度数法): 0=正面、負=左、正=右、±180=背面、None=LFE(常にgain=1.0)
# 各エントリはFFmpegのデフォルトチャンネルレイアウト順に準拠
_CHANNEL_AZIMUTHS: dict[int, list[float | None]] = {
    1: [0.0],                                                    # Mono: FC
    2: [-30.0, 30.0],                                            # Stereo: FL FR
    3: [-30.0, 30.0, 0.0],                                       # 3.0: FL FR FC
    4: [-30.0, 30.0, -110.0, 110.0],                            # 4.0: FL FR BL BR
    5: [-30.0, 30.0, 0.0, -110.0, 110.0],                       # 5.0: FL FR FC BL BR
    6: [-30.0, 30.0, 0.0, None, -110.0, 110.0],                 # 5.1: FL FR FC LFE BL BR
    7: [-30.0, 30.0, 0.0, None, -110.0, 110.0, 180.0],         # 6.1: FL FR FC LFE BL BR BC
    8: [-30.0, 30.0, 0.0, None, -110.0, 110.0, -60.0, 60.0],  # 7.1: FL FR FC LFE BL BR SL SR
}


def _compute_pan_gains(pan: float, channels: int) -> npt.NDArray[np.float32]:
    """pan [-1, 1] をスピーカーアジマスに基づくチャンネルごとのゲインに変換する。
    未定義チャンネル数の場合はすべて 1.0 を返す。"""
    azimuths = _CHANNEL_AZIMUTHS.get(channels)
    if azimuths is None:
        return np.ones(channels, dtype=np.float32)
    pan_rad = math.radians(pan * 90.0)
    gains = [
        1.0 if az is None else max(0.0, math.cos(pan_rad - math.radians(az)))
        for az in azimuths
    ]
    return np.array(gains, dtype=np.float32)


def mix(
    output: npt.NDArray[np.float32],
    channels: int,
    sample_count: int,
    obj_name: str,
    expected_count: int,
    samples: npt.NDArray[np.float32],
    volume: float,
    pan: float,
    sample_offset: int,
) -> None:
    """`samples`にボリューム・パンを適用し、`output`(呼び出し元が保持する出力バッファ)に
    in-placeで加算ミックスする。"""
    if samples.ndim != 2 or samples.shape[0] != channels:
        raise ValueError(
            f"Audio plugin {obj_name} returned samples with invalid shape {samples.shape}, expected ({channels}, {expected_count})"
        )
    pan_gains = _compute_pan_gains(pan / 100.0, channels)
    mixed = samples * (volume / 100.0 * pan_gains[:, np.newaxis])
    write_count = min(expected_count, sample_count - sample_offset)
    output[:, sample_offset:sample_offset + write_count] += mixed[:, :write_count]

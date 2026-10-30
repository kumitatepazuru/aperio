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


class AudioMixBuffer:
    """1回のオーディオ生成リクエスト分(チャンネル数 x サンプル数)の出力バッファ。
    各アイテムのサンプルにボリューム・パンを適用して加算ミックスしていく。"""

    def __init__(self, channels: int, sample_count: int) -> None:
        self.channels = channels
        self.sample_count = sample_count
        self._output = np.zeros((channels, sample_count), dtype=np.float32)

    def add(
        self,
        source_name: str,
        samples: npt.NDArray[np.float32],
        expected_count: int,
        volume: float,
        pan: float,
        sample_offset: int,
    ) -> None:
        """`samples`にボリューム・パンを適用し、バッファの`sample_offset`以降にin-placeで加算ミックスする。

        Args:
            source_name: サンプルを生成したジェネレーター名 (エラーメッセージ用)
            samples: (channels, expected_count) のサンプル
            expected_count: 書き込むサンプル数
            volume: ボリューム (0-100)
            pan: パン (-100 - 100)
            sample_offset: バッファ内の書き込み開始位置
        """
        if samples.ndim != 2 or samples.shape[0] != self.channels:
            raise ValueError(
                f"Audio plugin {source_name} returned samples with invalid shape {samples.shape}, expected ({self.channels}, {expected_count})"
            )
        pan_gains = _compute_pan_gains(pan / 100.0, self.channels)
        mixed = samples * (volume / 100.0 * pan_gains[:, np.newaxis])
        write_count = min(expected_count, self.sample_count - sample_offset)
        self._output[:, sample_offset:sample_offset + write_count] += mixed[:, :write_count]

    def finish(self) -> npt.NDArray[np.float32]:
        """浮動小数点丸め誤差などによる境界超過を[-1, 1]にクリップしてバッファを返す。"""
        np.clip(self._output, -1.0, 1.0, out=self._output)
        return self._output

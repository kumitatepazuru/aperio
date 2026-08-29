import numpy as np
import numpy.typing as npt

def mix(output: npt.NDArray[np.float32], channels: int, sample_count: int, obj_name: str, expected_count: int, samples: npt.NDArray[np.float32], volume: float, pan: float, sample_offset: int) -> None:
    """`samples`にボリューム・パンを適用し、`output`(呼び出し元が保持する出力バッファ)に
    in-placeで加算ミックスする。"""

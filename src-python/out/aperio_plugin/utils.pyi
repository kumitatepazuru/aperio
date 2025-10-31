import numpy as np
import taichi as ti
from _typeshed import Incomplete

taichi_dtype_map: Incomplete

def check_array_shape(arr: np.ndarray | ti.Field, expected_shape: tuple[int, ...], expected_dtype: None, name: str = 'Base') -> None:
    """
    arrの形状とデータ型が期待通りか確認するユーティリティ関数。
    もし異なっていた場合はValueErrorを投げる。
    """

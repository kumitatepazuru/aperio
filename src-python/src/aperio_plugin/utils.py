import numpy as np
import taichi as ti
    
taichi_dtype_map = {
    np.uint8: ti.u8,
    np.int32: ti.i32,
    np.float32: ti.f32,
    np.float64: ti.f64,
}

def check_array_shape(arr: np.ndarray | ti.Field, expected_shape: tuple[int, ...], expected_dtype: taichi_dtype_map.keys(), name: str = "Base") -> None:
    """
    arrの形状とデータ型が期待通りか確認するユーティリティ関数。
    もし異なっていた場合はValueErrorを投げる。
    """

    # 形状の確認
    if arr.shape != expected_shape:
        raise ValueError(f"Array shape {arr.shape} does not match expected shape {expected_shape}")

    # データ型の確認
    if isinstance(arr, np.ndarray):
        if arr.dtype != expected_dtype:
            raise ValueError(f"At {name}: Numpy array dtype {arr.dtype} does not match expected dtype {expected_dtype}")
    elif isinstance(arr, ti.Field):
        if arr.dtype != taichi_dtype_map[expected_dtype]:
            raise ValueError(f"At {name}: Taichi field dtype {arr.dtype} does not match expected dtype {taichi_dtype_map[expected_dtype]}")
    else:
        raise TypeError(f"Unsupported array type: {type(arr)}")
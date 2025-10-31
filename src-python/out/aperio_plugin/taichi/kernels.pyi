import taichi as ti

@ti.kernel
def composite_layer(final_frame: None, layer_frame: None, x_offset: ti.i32, y_offset: ti.i32, channels: None):
    """
    指定されたレイヤーを最終フレームに合成（ブレンディング）するTaichiカーネル。
    クリッピングもこのカーネル内で行う。
    """
@ti.kernel
def normalize_from_numpy(field: None, np_array: None):
    """
    uint8のNumpy配列を、f32のTaichiフィールドに正規化しながらコピーする。
    """
@ti.kernel
def finalize_to_numpy(field: None, np_array: None):
    """
    f32のTaichiフィールドを、uint8のNumpy配列に非正規化・クリップしながら書き出す。
    """

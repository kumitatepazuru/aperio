import taichi as ti
import numpy as np

# Taichiの初期化 (アプリケーションのどこか一度だけ実行)
# ti.init(arch=ti.gpu) # GPUを使う場合
# ti.init(arch=ti.cpu) # CPUを使う場合

@ti.kernel
def composite_layer(
    final_frame: ti.template(),
    layer_frame: ti.template(),
    x_offset: ti.i32,
    y_offset: ti.i32,
    channels: ti.template(), # ti.staticで使うためtemplateとして渡す
):
    """
    指定されたレイヤーを最終フレームに合成（ブレンディング）するTaichiカーネル。
    クリッピングもこのカーネル内で行う。
    """
    # layer_frameの全ピクセルに対して並列で処理を実行
    for row, col in layer_frame:
        target_x = x_offset + col
        target_y = y_offset + row

        # ターゲット座標がfinal_frameの範囲内かチェック（クリッピング）
        if 0 <= target_y < final_frame.shape[0] and 0 <= target_x < final_frame.shape[1]:
            
            # ti.static() を使うことで、コンパイル時に不要な分岐が削除され、高速になる
            if ti.static(channels == 4):
                # RGBA: アルファブレンディング
                layer_color = ti.Vector([layer_frame[row, col][0], layer_frame[row, col][1], layer_frame[row, col][2]])
                alpha = layer_frame[row, col][3]
                
                base_color = final_frame[target_y, target_x]
                final_color = alpha * layer_color + (1.0 - alpha) * base_color
                final_frame[target_y, target_x] = final_color

            elif ti.static(channels == 3):
                # RGB: そのまま上書き
                final_frame[target_y, target_x] = layer_frame[row, col]

            elif ti.static(channels == 1):
                # Grayscale: 3チャンネルに拡張して上書き
                gray_val = layer_frame[row, col]
                final_frame[target_y, target_x] = ti.Vector([gray_val, gray_val, gray_val])

@ti.kernel
def normalize_from_numpy(field: ti.template(), np_array: ti.types.ndarray()):
    """
    uint8のNumpy配列を、f32のTaichiフィールドに正規化しながらコピーする。
    """
    for I in ti.grouped(np_array):
        # np_arrayの次元数に関わらず動作する
        field[I] = np_array[I].cast(ti.f32) / 255.0

@ti.kernel
def finalize_to_numpy(field: ti.template(), np_array: ti.types.ndarray()):
    """
    f32のTaichiフィールドを、uint8のNumpy配列に非正規化・クリップしながら書き出す。
    """
    for I in ti.grouped(np_array):
        # 0.0-1.0の範囲を超えた値をクリップ
        clipped_val = ti.max(0.0, ti.min(1.0, field[I]))
        np_array[I] = (clipped_val * 255.0).cast(ti.u8) # type: ignore

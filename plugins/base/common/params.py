import struct

from aperio.item_structures import GeneratorInformation, RequestStructureParameter


def clamp(value, lo, hi):
    return max(lo, min(hi, value))


def make_generator_information(
    display_name: str, structure: list[RequestStructureParameter]
) -> GeneratorInformation:
    """on_request_structure の定型 return
    (duration_frames/max_frame/min_frame は常に None)をまとめる。"""
    return GeneratorInformation(
        display_name=display_name,
        duration_frames=None,
        max_frame=None,
        min_frame=None,
        structure=structure,
    )


def pack_expand_params(
    off_x: int, off_y: int, new_w: int, new_h: int, border: tuple[float, float, float, float] = (0.0, 0.0, 0.0, 0.0)
) -> bytes:
    """expand.wgsl 用パラメータ。"""
    return struct.pack("iiiiffff", off_x, off_y, new_w, new_h, *border)


def pack_box_average_dir_params(radius: int, step_x: int, step_y: int, w: int, h: int) -> bytes:
    """box_average_dir.wgsl 用パラメータ。"""
    return struct.pack("iiiii", radius, step_x, step_y, w, h)


def split_radius(radius: int) -> tuple[int, int]:
    """半径を2つのボックスパスに分割する(r_hi=ceil(r/2), r_lo=floor(r/2))。合成
    カーネルが三角形(半径が奇数なら上底3の台形)になる、実機ぼかし系フィルタの
    分割方法。`ぼかし`(blur.py)と`シャープ`(sharp.py)が共有する。"""
    return radius - radius // 2, radius // 2


def pack_box_blur_dir_params(
    radius: int, step_x: int, step_y: int, w: int, h: int, offset: int = 0, border_mode: int = 1, divisor_mode: int = 0
) -> bytes:
    """box_blur_dir.wgsl 用パラメータ。"""
    return struct.pack("iiiiiiii", radius, step_x, step_y, w, h, offset, border_mode, divisor_mode)

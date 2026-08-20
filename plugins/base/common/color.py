def bt601_encode(r: float, g: float, b: float) -> tuple[float, float, float]:
    """common/lib/color.wgsl の bt601_encode と同じ式(1画素分だけをPython側で
    事前計算するために使う)。戻り値は (cb, cr, y)。"""
    y = 0.299 * r + 0.587 * g + 0.114 * b
    cr = (r - y) / 1.402
    cb = (b - y) / 1.772
    return cb, cr, y

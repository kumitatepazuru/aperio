def ab_constant(r: int) -> float:
    """境界補正の伸張定数(コントラスト伸張カーブ t(v) = (v - a) * r の係数)。
    a = 1 - 1/r で、v=1(完全不透明)のとき厳密に t(1) = 1 になる。"""
    return 1.0 - 1.0 / r

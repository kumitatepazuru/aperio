def ab_constants(r: int) -> tuple[float, float]:
    """境界補正のA/B定数(README §5/§7)。A = 4096 - 4096/r, B = 4096 - (4096/r)*r
    (いずれも整数除算)。r は 1〜5 の整数なので Python の floor division で
    丸め誤差なく再現できる。"""
    a_int = 4096 - 4096 // r
    b_int = 4096 - (4096 // r) * r
    return a_int / 4096.0, b_int / 4096.0

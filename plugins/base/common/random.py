import random


def _seed_key(*parts) -> str:
    return ":".join(str(p) for p in parts)


def rand_unit(*parts) -> float:
    """parts から一意に定まる [0,1) の疑似乱数を1つ返す(同じ parts なら常に同じ値)。"""
    return random.Random(_seed_key(*parts)).random()


def seed_u32(*parts) -> int:
    """parts から一意に定まる32bit符号なし整数を返す(WGSL側の乱数シードとして渡す用)。"""
    return random.Random(_seed_key(*parts)).getrandbits(32)

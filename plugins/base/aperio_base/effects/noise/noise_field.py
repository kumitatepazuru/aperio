import functools
import math

# exedit-inspect noise README §4: `ノイズ`の func_init が作る256x256の乱数場を
# 再現する。種が固定なので結果は常にプロジェクトに依存せず同じになる。

_SEED0 = 0x12345678
_LCG_MULTIPLIER = 73
_LCG_XOR_CONSTANT = 0x137BD137
_LCG_MODULUS = 7213  # README記載の値。元コードでは0x1c2d
_FIELD_SIZE = 256  # 乱数場は256x256


def _wrap_to_int32(value: int) -> int:
    """Pythonの整数は多倍長なので、Cのint32演算が桁あふれで折り返す挙動を
    明示的に再現する(符号付き32bit整数として解釈し直すだけ)。"""
    value &= 0xFFFFFFFF
    return value - 0x1_0000_0000 if value & 0x8000_0000 else value


def _advance_seed(seed: int) -> int:
    """乱数を1ステップ進める(exedit-inspect noise README §4)。"""
    scrambled = _wrap_to_int32(_wrap_to_int32(seed * _LCG_MULTIPLIER) ^ _LCG_XOR_CONSTANT)
    # Pythonの`%`は負数の扱いがCと異なる(常に非負を返す)ため、Cと同じ
    # 「符号は被除数に従う」丸めをするmath.fmodを使う。
    remainder = int(math.fmod(seed, _LCG_MODULUS))
    seed = _wrap_to_int32(_wrap_to_int32(remainder + seed) + scrambled)
    mixed = _wrap_to_int32(_wrap_to_int32(seed << 7) ^ (seed >> 2))
    return _wrap_to_int32(seed ^ mixed)


def _generate_raw_values() -> list[int]:
    values = []
    seed = _SEED0
    for _ in range(_FIELD_SIZE * _FIELD_SIZE):
        seed = _advance_seed(seed)
        values.append(seed & 0xFFFFFF)  # 下位24bitだけを使う
    return values


def _smooth_3x3(values: list[int]) -> list[int]:
    """3x3の二項フィルタ(重み 1 2 1 / 2 4 2 / 1 2 1)。行・列とも256でラップする。"""
    smoothed = [0] * len(values)
    for row in range(_FIELD_SIZE):
        row_above = ((row - 1) % _FIELD_SIZE) * _FIELD_SIZE
        row_here = row * _FIELD_SIZE
        row_below = ((row + 1) % _FIELD_SIZE) * _FIELD_SIZE
        for col in range(_FIELD_SIZE):
            col_left = (col - 1) % _FIELD_SIZE
            col_right = (col + 1) % _FIELD_SIZE
            smoothed[row_here + col] = (
                values[row_above + col_left] + 2 * values[row_above + col] + values[row_above + col_right]
                + 2 * values[row_here + col_left] + 4 * values[row_here + col] + 2 * values[row_here + col_right]
                + values[row_below + col_left] + 2 * values[row_below + col] + values[row_below + col_right]
            )
    return smoothed


def _normalize(values: list[int]) -> list[float]:
    """[-2048, +2048]へ写像する(README §4)。"""
    lo, hi = min(values), max(values)
    return [-2048.0 + math.trunc((v - lo) * 4096.0 / (hi - lo)) for v in values]


@functools.lru_cache(maxsize=1)
def build_noise_field() -> tuple[float, ...]:
    """種が固定なので結果は常に同じ ―― プロセス内で1回だけ計算してキャッシュする。"""
    return tuple(_normalize(_smooth_3x3(_generate_raw_values())))

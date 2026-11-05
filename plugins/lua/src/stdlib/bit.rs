//! bit ライブラリ(lib_bit.c、LuaJIT 固有)。引数を 32 bit 整数に正規化して演算する。

use super::{f, Builtin, Class::*, Effect::*, GpuImpl::*};

pub(super) const ENTRIES: &[Builtin] = &[
    f("bit.tobit", B, Pure, Helper("lg_tobit"), "normalizes a number to a signed 32-bit integer (modulo 2^32)"),
    f("bit.bnot", B, Pure, Helper("lg_bnot"), ""),
    f("bit.band", B, Pure, Helper("lg_band"), "n-ary"),
    f("bit.bor", B, Pure, Helper("lg_bor"), "n-ary"),
    f("bit.bxor", B, Pure, Helper("lg_bxor"), "n-ary"),
    f("bit.lshift", B, Pure, Helper("lg_lshift"), "shift count is taken modulo 32"),
    f("bit.rshift", B, Pure, Helper("lg_rshift"), "logical shift"),
    f("bit.arshift", B, Pure, Helper("lg_arshift"), "arithmetic shift"),
    f("bit.rol", B, Pure, Helper("lg_rol"), ""),
    f("bit.ror", B, Pure, Helper("lg_ror"), ""),
    f("bit.bswap", B, Pure, Helper("lg_bswap"), ""),
    f("bit.tohex", C, Pure, None, "builds a string"),
];

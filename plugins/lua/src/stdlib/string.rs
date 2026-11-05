//! string ライブラリ(lib_string.c)。文字列は GPU に持ち込めないので基本は C。

use super::{f, Builtin, Class::*, Effect::*, GpuImpl::*};

pub(super) const ENTRIES: &[Builtin] = &[
    f("string.len", B, Pure, Special, "constant strings are folded; otherwise CPU"),
    f("string.byte", B, Pure, Special,
      "constant strings become a constant array (index may be varying); otherwise CPU"),
    f("string.char", C, Pure, None, "builds a string"),
    f("string.dump", C, Opaque, None, "bytecode of a function"),
    f("string.find", C, Pure, None, "Lua patterns"),
    f("string.format", C, Pure, None, "C printf-like formatting"),
    f("string.gmatch", C, Stream, None, "returns a stateful iterator"),
    f("string.gsub", C, Pure, None, "Lua patterns"),
    f("string.lower", C, Pure, None, ""),
    f("string.upper", C, Pure, None, ""),
    f("string.match", C, Pure, None, "Lua patterns"),
    f("string.rep", C, Pure, None, "LuaJIT accepts a separator (5.2 extension)"),
    f("string.reverse", C, Pure, None, ""),
    f("string.sub", C, Pure, None, ""),
];

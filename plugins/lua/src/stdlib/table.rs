//! table ライブラリ(lib_table.c)。

use super::{f, Builtin, Class::*, Effect::*, GpuImpl::*};

pub(super) const ENTRIES: &[Builtin] = &[
    f("table.maxn", B, Pure, Special, "static length for arrays known on the GPU; otherwise CPU"),
    f("table.getn", B, Pure, Special, "same as #t"),
    f("table.foreach", B, Pure, Special, "deprecated; arrays only on the GPU (function is inlined)"),
    f("table.foreachi", B, Pure, Special, "deprecated; function is inlined"),
    f("table.sort", B, Write, Special,
      "numeric arrays of static length use an embedded insertion sort; comparator must be inlinable"),
    f("table.insert", B, Write, Special, "private local arrays inside a kernel; otherwise CPU"),
    f("table.remove", B, Write, Special, "private local arrays inside a kernel; otherwise CPU"),
    f("table.concat", C, Pure, None, "builds a string"),
    f("table.move", C, Write, None, "LuaJIT 2.1 extension: copies a range into another table"),
];

//! os ライブラリ(lib_os.c)。

use super::{f, Builtin, Class::*, Effect::*, GpuImpl::*};

pub(super) const ENTRIES: &[Builtin] = &[
    f("os.clock", C, Stream, None, "CPU time; changes between calls, so it is replayed (not hoisted)"),
    f("os.time", C, Stream, None, "current time without arguments; with a table argument it is a pure conversion"),
    f("os.date", C, Stream, None, "uses the current time unless a time is given"),
    f("os.getenv", C, ReadOnly, None, ""),
    f("os.difftime", B, Pure, Helper("lg_difftime"), "t2 - t1"),
    f("os.execute", C, Write, None, ""),
    f("os.exit", C, Write, None, ""),
    f("os.remove", C, Write, None, ""),
    f("os.rename", C, Write, None, ""),
    f("os.tmpname", C, Write, None, ""),
    f("os.setlocale", C, Write, None, ""),
];

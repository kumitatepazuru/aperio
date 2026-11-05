//! debug ライブラリ(lib_debug.c)。すべて C。

use super::{f, Builtin, Class::*, Effect::*, GpuImpl::*};

pub(super) const ENTRIES: &[Builtin] = &[
    f("debug.debug", C, Opaque, None, ""),
    f("debug.getfenv", C, Opaque, None, ""),
    f("debug.gethook", C, Opaque, None, ""),
    f("debug.getinfo", C, Opaque, None, ""),
    f("debug.getlocal", C, Opaque, None, ""),
    f("debug.getmetatable", C, Opaque, None, ""),
    f("debug.getregistry", C, Opaque, None, ""),
    f("debug.getupvalue", C, Opaque, None, ""),
    f("debug.setfenv", C, Opaque, None, ""),
    f("debug.sethook", C, Opaque, None, ""),
    f("debug.setlocal", C, Opaque, None, ""),
    f("debug.setmetatable", C, Opaque, None, ""),
    f("debug.setupvalue", C, Opaque, None, ""),
    f("debug.traceback", C, Opaque, None, ""),
    f("debug.upvalueid", C, Opaque, None, ""),
    f("debug.upvaluejoin", C, Opaque, None, ""),
];

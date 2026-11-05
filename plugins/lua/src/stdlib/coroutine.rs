//! coroutine ライブラリ(lib_base.c 内)。すべて C。

use super::{f, Builtin, Class::*, Effect::*, GpuImpl::*};

pub(super) const ENTRIES: &[Builtin] = &[
    f("coroutine.create", C, Opaque, None, ""),
    f("coroutine.resume", C, Opaque, None, ""),
    f("coroutine.running", C, Opaque, None, ""),
    f("coroutine.status", C, Opaque, None, ""),
    f("coroutine.wrap", C, Opaque, None, ""),
    f("coroutine.yield", C, Opaque, None, ""),
    f("coroutine.isyieldable", C, Opaque, None, "LuaJIT 2.1 extension"),
];

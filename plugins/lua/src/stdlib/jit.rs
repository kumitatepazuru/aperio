//! jit ライブラリ(lib_jit.c、LuaJIT 固有)。すべて C。

use super::{f, v, Builtin, Class::*, Effect::*, GpuImpl::*};

pub(super) const ENTRIES: &[Builtin] = &[
    f("jit.on", C, Write, None, ""),
    f("jit.off", C, Write, None, ""),
    f("jit.flush", C, Write, None, ""),
    f("jit.status", C, ReadOnly, None, ""),
    f("jit.attach", C, Opaque, None, ""),
    f("jit.security", C, ReadOnly, None, ""),
    v("jit.arch", C, Pure, None, ""),
    v("jit.os", C, Pure, None, ""),
    v("jit.version", C, Pure, None, ""),
    v("jit.version_num", C, Pure, None, ""),
    v("jit.opt", C, Opaque, None, ""),
];

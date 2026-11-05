//! ffi ライブラリ(lib_ffi.c、LuaJIT 固有)。C の型・メモリを扱うので、すべて C。

use super::{f, v, Builtin, Class::*, Effect::*, GpuImpl::*};

pub(super) const ENTRIES: &[Builtin] = &[
    f("ffi.cdef", C, Opaque, None, ""),
    f("ffi.load", C, Opaque, None, ""),
    f("ffi.new", C, Opaque, None, ""),
    f("ffi.cast", C, Opaque, None, ""),
    f("ffi.typeof", C, Opaque, None, ""),
    f("ffi.metatype", C, Opaque, None, ""),
    f("ffi.gc", C, Opaque, None, ""),
    f("ffi.sizeof", C, Opaque, None, ""),
    f("ffi.alignof", C, Opaque, None, ""),
    f("ffi.offsetof", C, Opaque, None, ""),
    f("ffi.istype", C, Opaque, None, ""),
    f("ffi.errno", C, Opaque, None, ""),
    f("ffi.string", C, Opaque, None, ""),
    f("ffi.copy", C, Opaque, None, ""),
    f("ffi.fill", C, Opaque, None, ""),
    f("ffi.abi", C, Opaque, None, ""),
    f("ffi.typeinfo", C, Opaque, None, ""),
    v("ffi.C", C, Opaque, None, ""),
    v("ffi.os", C, Pure, None, ""),
    v("ffi.arch", C, Pure, None, ""),
];

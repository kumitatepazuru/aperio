//! package ライブラリ(lib_package.c)。すべて C。

use super::{f, v, Builtin, Class::*, Effect::*, GpuImpl::*};

pub(super) const ENTRIES: &[Builtin] = &[
    f("package.loadlib", C, Opaque, None, "loads a C library"),
    f("package.seeall", C, Opaque, None, "module helper"),
    f("package.searchpath", C, ReadOnly, None, "LuaJIT extension (5.2)"),
    v("package.path", C, Opaque, None, ""),
    v("package.cpath", C, Opaque, None, ""),
    v("package.config", C, Opaque, None, ""),
    v("package.loaded", C, Opaque, None, ""),
    v("package.loaders", C, Opaque, None, ""),
    v("package.preload", C, Opaque, None, "bundled modules are registered here"),
];

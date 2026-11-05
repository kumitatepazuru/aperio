//! io ライブラリ(lib_io.c)と file オブジェクトのメソッド。すべて C。

use super::{f, v, Builtin, Class::*, Effect::*, GpuImpl::*};

pub(super) const ENTRIES: &[Builtin] = &[
    f("io.open", C, ReadOnly, None,
      "read modes (\"r\", \"rb\", default) are treated as a read; other modes as a write"),
    f("io.lines", C, Stream, None, "returns a stateful line iterator"),
    f("io.read", C, Stream, None, "reads from the default input (stdin)"),
    f("io.write", C, Write, None, "writes to the default output"),
    f("io.close", C, Write, None, ""),
    f("io.flush", C, Write, None, ""),
    f("io.input", C, Write, None, "changes the default input"),
    f("io.output", C, Write, None, "changes the default output"),
    f("io.popen", C, Write, None, "starts a process"),
    f("io.tmpfile", C, Write, None, "creates a file"),
    f("io.type", C, Pure, None, "\"file\" / \"closed file\" / nil"),
    v("io.stdin", C, Opaque, None, ""),
    v("io.stdout", C, Opaque, None, ""),
    v("io.stderr", C, Opaque, None, ""),
    // file オブジェクトのメソッド(レシーバーが file のとき)。
    f("file:read", C, Stream, None,
      "reads from the current position; `io.open(path):read(...)` on a fresh handle is a read"),
    f("file:lines", C, Stream, None, ""),
    f("file:write", C, Write, None, ""),
    f("file:seek", C, Write, None, "moves the file position"),
    f("file:setvbuf", C, Write, None, ""),
    f("file:flush", C, Write, None, ""),
    f("file:close", C, Write, None, ""),
];

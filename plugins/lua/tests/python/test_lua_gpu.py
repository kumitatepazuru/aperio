"""lua_gpu の Python バインディングのスモークテスト。

wheel をビルドしてから実行する:
    uvx maturin build --release -m plugins/lua/Cargo.toml -o build/plugins
    uv run --no-project --isolated --python 3.14 --with build/plugins/lua_gpu-*.whl python plugins/lua/tests/python/test_lua_gpu.py
"""

import struct

import lua_gpu
from lua_gpu import CpuFunction, Ext, Gen, GpuFunction, LuaGPU, PyFunction

calls: list[lua_gpu.CallSite] = []
log: list[tuple] = []


def gen_getpixel(c: lua_gpu.CallSite) -> Gen:
    calls.append(c)
    assert c.context == "kernel"
    return Gen(
        stmts=f"float4 c = inputs.tex0.Load(int3(int({c.args[0]}), int({c.args[1]}), 0)) * 255.0;",
        exprs=["c.r", "c.g", "c.b", "c.a"],
    )


def gen_putpixel(c: lua_gpu.CallSite) -> Gen:
    calls.append(c)
    a = c.args
    return Gen(stmts=f"res.outputTex[int2(int({a[0]}), int({a[1]}))] = float4({a[2]}, {a[3]}, {a[4]}, {a[5]}) / 255.0;")


def gen_option(c: lua_gpu.CallSite) -> str | None:
    calls.append(c)
    assert c.context == "cpu"
    if c.consts[0] != "track":
        return None  # 変換エラーになる
    return f"${{track}}[{c.args[1]} + 1]"


gpu = LuaGPU(
    [
        CpuFunction("obj.w", lambda c: "${w}", property=True, returns=["int"], externals=[Ext("w", "int")]),
        CpuFunction("obj.h", lambda c: "${h}", property=True, returns=["int"], externals=[Ext("h", "int")]),
        CpuFunction("obj.getoption", gen_option, returns=["float"], externals=[Ext("track", "float[2]")]),
        GpuFunction("obj.getpixel", gen_getpixel, params=["float", "float"], returns=["float"] * 4),
        GpuFunction("obj.putpixel", gen_putpixel, params=["float"] * 6, returns=[]),
        PyFunction("host.log", lambda *a: log.append(a) or (len(log), "ok")),
    ]
)

src = """
local n, s = host.log("start", 1.5, true, {1, 2})
assert(type(n) == "number" and s == "ok")
local gain = obj.getoption("track", 1)
for y = 0, obj.h - 1 do
  for x = 0, obj.w - 1 do
    local r, g, b, a = obj.getpixel(x, y)
    obj.putpixel(x, y, r * gain, g, b, a)
  end
end
"""

res = gpu.convert(src)
assert res is not None, gpu.last_diagnostics
assert len(res.shaders) == 1
assert "ParameterBlock<EffectResourcesSampler<LuaGpuWord>> res" in res.shaders[0]
assert res.lua.startswith("--[[lua_gpu v1 ")
assert [c.name for c in calls] == ["obj.getoption", "obj.getpixel", "obj.putpixel"]
assert calls[0].consts == ["track", 1.0] and calls[0].line == 4

ext = {"w": 4, "h": 2, "track": [0.5, 2.0]}
dispatches = res(ext)
assert len(dispatches) == 1
k, params = dispatches[0]
words = struct.unpack(f"<{len(params) // 4}I", params)
assert k == 0 and words[0] == 8, words
# gain(= track[2] = 2.0)は捕捉されて params に入る。
assert struct.pack("<f", 2.0) in params
assert log[0] == ("start", 1.5, True, [1.0, 2.0]), log

# on_dispatch では実行順にコールバックされ、戻り値は空。
seen = []
assert gpu.run(res.lua, ext, on_dispatch=lambda k, p: seen.append((k, len(p)))) == []
assert len(seen) == 1

# 外部値の検査。
for bad in ({"w": 4, "h": 2}, {"w": "x", "h": 2, "track": [1, 2]}, {"w": 4, "h": 2, "track": [1]}):
    try:
        res(bad)
    except lua_gpu.LuaGpuArgError:
        pass
    else:
        raise AssertionError(f"expected LuaGpuArgError for {bad}")

# Lua の実行時エラー。
bad_lua = gpu.convert("error('boom')")
assert bad_lua is not None
try:
    bad_lua(ext)
except lua_gpu.LuaGpuRuntimeError as e:
    # 元のチャンク名と行番号で報告される。
    assert "main.lua:1: boom" in str(e), str(e)
else:
    raise AssertionError("expected LuaGpuRuntimeError")

# 変換エラーは None と診断。
assert gpu.convert("local x = obj.getoption('other', 1)") is None
diags = gpu.last_diagnostics
assert diags[0]["severity"] == "error" and diags[0]["code"] == "generator-failed", diags
assert gpu.convert("local x = ") is None
assert gpu.last_diagnostics[0]["code"] == "syntax"

print("ok", lua_gpu.__version__)

// 標準ライブラリの分類表が、実際の LuaJIT ランタイムにあるもの(グローバル / 各ライブラリ /
// file メソッド)をすべて網羅し、かつ存在しないものを含まないことを検査する。

use std::collections::BTreeSet;

use lua_gpu::runtime::new_lua_state;
use lua_gpu::stdlib::{self, Class, GpuImpl, Kind};

const LIBS: &[&str] = &[
    "math", "string", "table", "io", "os", "package", "coroutine", "debug", "bit", "jit", "ffi",
];

fn runtime_names() -> BTreeSet<String> {
    let lua = new_lua_state();
    let names: Vec<String> = lua
        .load(
            r#"
            local out = {}
            local libs = { ... }
            local is_lib = {}
            for _, l in ipairs(libs) do is_lib[l] = true end
            for k in pairs(_G) do out[#out + 1] = k end
            for _, l in ipairs(libs) do
                for k in pairs(_G[l]) do out[#out + 1] = l .. "." .. k end
            end
            -- file のメソッド(メタメソッド __gc / __index / __tostring は呼び出す関数ではないので除く)。
            for k in pairs(getmetatable(io.stdout).__index) do
                if k:sub(1, 2) ~= "__" then out[#out + 1] = "file:" .. k end
            end
            return out
            "#,
        )
        .call::<Vec<String>>(mlua::MultiValue::from_iter(
            LIBS.iter().map(|l| mlua::Value::String(lua.create_string(l).unwrap())),
        ))
        .unwrap();
    names.into_iter().collect()
}

#[test]
fn table_matches_the_runtime_exactly() {
    let runtime = runtime_names();
    let table: BTreeSet<String> = stdlib::all().map(|b| b.name.to_string()).collect();
    let missing: Vec<_> = runtime.difference(&table).collect();
    let extra: Vec<_> = table.difference(&runtime).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "not classified: {missing:?}\nnot in the runtime: {extra:?}"
    );
}

#[test]
fn entries_are_consistent() {
    for b in stdlib::all() {
        match b.class {
            Class::A => assert!(
                matches!(b.gpu, GpuImpl::Intrinsic(_) | GpuImpl::Const(_)),
                "{}: class A must map to a Slang intrinsic or constant",
                b.name
            ),
            Class::B => assert!(
                matches!(b.gpu, GpuImpl::Helper(_) | GpuImpl::Special),
                "{}: class B must use a helper or special handling",
                b.name
            ),
            Class::C => assert_eq!(b.gpu, GpuImpl::None, "{}: class C has no GPU impl", b.name),
        }
        if b.kind == Kind::Library {
            assert!(!b.name.contains('.'), "{}", b.name);
        }
    }
    assert_eq!(stdlib::lookup("math.sin").unwrap().gpu, GpuImpl::Intrinsic("sin"));
    assert!(stdlib::is_global("print"));
    // mlua の StdLib::ALL では ffi もグローバルに読み込まれる。
    assert!(stdlib::is_global("ffi"));
    assert!(!stdlib::is_global("obj"));
}

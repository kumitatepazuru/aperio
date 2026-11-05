// 効果クラスの解析: 組み込みの効果、引数による違い(io.open / os.time)、ユーザー関数への伝播、spec。

mod support;

use lua_gpu::analysis::effects::{Effects, SiteImpl};
use lua_gpu::analysis::resolve::Resolver;
use lua_gpu::diag::Diagnostics;
use lua_gpu::frontend;
use lua_gpu::hir::*;
use lua_gpu::stdlib::Effect;

/// 最後の文の効果。
fn last_effect(src: &str) -> Effect {
    let reg = support::obj_registry();
    let mut d = Diagnostics::new();
    let fe = frontend::build(src, "main.lua", None, &reg, &mut d).unwrap();
    assert!(!d.has_errors(), "{d}");
    let p = fe.program;
    let sites: Vec<SiteImpl> = p
        .sites
        .iter()
        .map(|s| match s.name.as_str() {
            "debug_log" => SiteImpl::Py,
            "obj.getpixel" | "obj.putpixel" => SiteImpl::Gpu,
            _ => SiteImpl::Unknown,
        })
        .collect();
    let r = Resolver::new(&p);
    let e = Effects::new(&p, &r, &sites);
    e.stmt(p.func(p.main_func()).body.stmts.last().unwrap())
}

#[test]
fn builtin_effects() {
    assert_eq!(last_effect("local x = math.sin(1) + math.floor(2.5)"), Effect::Pure);
    assert_eq!(last_effect("print(1)"), Effect::Write);
    assert_eq!(last_effect("local r = math.random()"), Effect::Stream);
    assert_eq!(last_effect("local s = string.format('%d', 1)"), Effect::Pure);
    assert_eq!(last_effect("local c = coroutine.create(print)"), Effect::Opaque);
}

#[test]
fn argument_dependent_effects() {
    assert_eq!(last_effect("local f = io.open('a.txt')"), Effect::ReadOnly);
    assert_eq!(last_effect("local f = io.open('a.txt', 'rb')"), Effect::ReadOnly);
    assert_eq!(last_effect("local f = io.open('a.txt', 'w')"), Effect::Write);
    assert_eq!(last_effect("local s = io.open('a.txt'):read('*a')"), Effect::ReadOnly);
    assert_eq!(last_effect("local t = os.time()"), Effect::Stream);
    assert_eq!(last_effect("local t = os.time({year = 2000, month = 1, day = 1})"), Effect::Pure);
}

#[test]
fn user_functions_propagate_effects() {
    assert_eq!(last_effect("local function f(x) return x * 2 end\nlocal y = f(3)"), Effect::Pure);
    assert_eq!(last_effect("local function f() print('x') end\nf()"), Effect::Write);
    // 再帰は解析できないので Opaque。
    assert_eq!(
        last_effect("local function f(n) if n > 0 then return f(n - 1) end end\nf(3)"),
        Effect::Opaque
    );
    // 中身の分からない関数値の呼び出しも Opaque。
    assert_eq!(last_effect("local g = select(1, print)\ng()"), Effect::Opaque);
}

#[test]
fn spec_effects() {
    assert_eq!(last_effect("local r = obj.getpixel(1, 2)"), Effect::Pure);
    assert_eq!(last_effect("debug_log(1)"), Effect::Stream);
    // generic for: ipairs / pairs は既知、それ以外は Opaque。
    assert_eq!(last_effect("for i, v in ipairs({1}) do end"), Effect::Pure);
    assert_eq!(last_effect("for l in io.lines('x') do end"), Effect::Opaque);
}

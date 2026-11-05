// 呼び出し先の静的解決: 組み込み、local の別名、グローバル関数、local テーブルのフィールド、モジュール。

mod support;

use std::fs;

use lua_gpu::analysis::resolve::{Resolver, StaticValue};
use lua_gpu::diag::Diagnostics;
use lua_gpu::frontend;
use lua_gpu::hir::*;

fn program(src: &str) -> Program {
    let reg = support::registry(vec![]);
    let mut d = Diagnostics::new();
    let fe = frontend::build(src, "main.lua", None, &reg, &mut d).unwrap();
    assert!(!d.has_errors(), "{d}");
    fe.program
}

/// 最後の文 `return <expr>` の式を解決する。
fn resolve_last(p: &Program) -> Option<StaticValue> {
    let r = Resolver::new(p);
    let StmtKind::Return(es) = &p.func(p.main_func()).body.stmts.last().unwrap().kind else {
        panic!()
    };
    r.value_of(&es[0])
}

fn builtin_name(v: Option<StaticValue>) -> Option<&'static str> {
    match v {
        Some(StaticValue::Builtin(b)) => Some(b.name),
        _ => None,
    }
}

#[test]
fn resolves_builtins_and_aliases() {
    assert_eq!(builtin_name(resolve_last(&program("return math.sin"))), Some("math.sin"));
    assert_eq!(builtin_name(resolve_last(&program("return print"))), Some("print"));
    assert_eq!(
        builtin_name(resolve_last(&program("local s = math.sqrt\nlocal t = s\nreturn t"))),
        Some("math.sqrt")
    );
    // 書き換えられたものは解決しない。
    assert_eq!(resolve_last(&program("math.sin = nil\nreturn math.sin")), None);
    assert_eq!(resolve_last(&program("print = nil\nreturn print")), None);
    assert_eq!(resolve_last(&program("local s = math.sin\ns = 1\nreturn s")), None);
}

#[test]
fn resolves_user_functions() {
    let p = program("local function f() end\nreturn f");
    assert!(matches!(resolve_last(&p), Some(StaticValue::Func(_))));
    let p = program("function g() end\nreturn g");
    assert!(matches!(resolve_last(&p), Some(StaticValue::Func(_))));
    // 2 回代入されるグローバルは解決しない。
    let p = program("function g() end\nfunction g() end\nreturn g");
    assert_eq!(resolve_last(&p), None);
}

#[test]
fn resolves_local_table_fields() {
    let p = program("local M = {}\nfunction M.f() end\nreturn M.f");
    assert!(matches!(resolve_last(&p), Some(StaticValue::Func(_))));
    let p = program("local M = { g = function() end }\nreturn M.g");
    assert!(matches!(resolve_last(&p), Some(StaticValue::Func(_))));
    // コンストラクタと後からの代入の両方があると解決しない。
    let p = program("local M = { g = function() end }\nM.g = function() end\nreturn M.g");
    assert_eq!(resolve_last(&p), None);
}

#[test]
fn resolves_module_fields() {
    let dir = std::env::temp_dir().join(format!("lua_gpu_resolve_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("util.lua"), "local M = {}\nfunction M.twice(x) return x * 2 end\nreturn M").unwrap();
    let reg = support::registry(vec![]);
    let mut d = Diagnostics::new();
    let fe = frontend::build(
        "local u = require('util')\nreturn u.twice",
        "main.lua",
        Some(&dir),
        &reg,
        &mut d,
    )
    .unwrap();
    let Some(StaticValue::Func(f)) = resolve_last(&fe.program) else { panic!() };
    assert_eq!(fe.program.func(f).name.as_deref(), Some("M.twice"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn dynamic_globals_disable_resolution() {
    let p = program("setfenv(1, {})\nreturn math.sin");
    assert!(Resolver::new(&p).has_dynamic_globals());
    assert_eq!(resolve_last(&p), None);
    let p = program("_G.x = 1\nreturn print");
    assert_eq!(resolve_last(&p), None);
}

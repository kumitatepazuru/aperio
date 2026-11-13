// フロントエンド: 定数パスの require / dofile を変換時に解決して読み込む。

mod support;

use std::fs;
use std::path::PathBuf;

use lua_gpu::diag::Diagnostics;
use lua_gpu::frontend;
use lua_gpu::hir::SourceKind;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lua_gpu_{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("lib")).unwrap();
    dir
}

#[test]
fn resolves_require_and_dofile() {
    let dir = temp_dir("modules");
    fs::write(dir.join("lib").join("util.lua"), "local M = {}\nfunction M.f(x) return x * 2 end\nreturn M").unwrap();
    // util が自分自身を require しても(循環)無限ループにならない。
    fs::write(dir.join("helper.lua"), "local u = require('lib.util')\nreturn u").unwrap();
    fs::write(dir.join("conf.lua"), "return 42").unwrap();

    let reg = support::registry(vec![]);
    let mut diags = Diagnostics::new();
    let fe = frontend::build(
        "local h = require('helper')\nlocal u = require 'lib.util'\nlocal c = dofile('conf.lua')",
        "main.lua",
        Some(&dir),
        &reg,
        &mut diags,
    )
    .unwrap();
    assert!(!diags.has_errors(), "{diags}");
    let p = &fe.program;
    assert_eq!(p.files.len(), 4);
    assert_eq!(fe.asts.len(), 4);
    let util = p.modules["lib.util"];
    assert_eq!(p.files[util as usize].kind, SourceKind::Module);
    assert!(p.modules.contains_key("helper"));
    let conf = p.dofiles["conf.lua"];
    assert_eq!(p.files[conf as usize].kind, SourceKind::Dofile);
    assert!(p.func(p.files[conf as usize].main).is_chunk);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn missing_modules_are_warnings() {
    let reg = support::registry(vec![]);
    let mut diags = Diagnostics::new();
    let fe = frontend::build("local m = require('nope')", "main.lua", None, &reg, &mut diags).unwrap();
    assert!(!diags.has_errors());
    assert_eq!(diags.warnings().next().unwrap().code, "module-not-found");
    assert!(fe.program.modules.is_empty());
}

#[test]
fn syntax_errors_in_modules_are_reported() {
    let dir = temp_dir("bad_module");
    fs::write(dir.join("bad.lua"), "return (").unwrap();
    let reg = support::registry(vec![]);
    let mut diags = Diagnostics::new();
    frontend::build("require('bad')", "main.lua", Some(&dir), &reg, &mut diags);
    let e = diags.errors().next().expect("error");
    assert_eq!(e.location.chunk, "bad");
    fs::remove_dir_all(&dir).ok();
}

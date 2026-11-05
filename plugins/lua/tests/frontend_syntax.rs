// フロントエンド: LuaJIT の構文(goto / ラベル / cdata 数値 / エスケープ)と構文エラーの診断。

mod support;

use lua_gpu::diag::Diagnostics;
use lua_gpu::frontend::{self, literal};
use lua_gpu::hir::*;

fn build(src: &str) -> (Option<Program>, Diagnostics) {
    let reg = support::registry(vec![]);
    let mut diags = Diagnostics::new();
    let fe = frontend::build(src, "test.lua", None, &reg, &mut diags);
    (fe.map(|f| f.program), diags)
}

fn first_expr(p: &Program) -> &ExprKind {
    match &p.func(p.main_func()).body.stmts[0].kind {
        StmtKind::Local { exprs, .. } => &exprs[0].kind,
        k => panic!("{k:?}"),
    }
}

#[test]
fn parses_goto_and_labels() {
    let (p, d) = build("for i = 1, 3 do if i == 2 then goto continue end ::continue:: end");
    assert!(!d.has_errors(), "{d}");
    let p = p.unwrap();
    let StmtKind::NumFor { body, .. } = &p.func(p.main_func()).body.stmts[0].kind else { panic!() };
    assert!(matches!(&body.stmts[1].kind, StmtKind::Label(l) if l == "continue"));
}

#[test]
fn numbers() {
    for (src, v) in [
        ("1", 1.0),
        ("3.5", 3.5),
        (".5", 0.5),
        ("1e3", 1000.0),
        ("0x10", 16.0),
        ("0b101", 5.0),
    ] {
        let (p, d) = build(&format!("local x = {src}"));
        assert!(!d.has_errors(), "{src}: {d}");
        assert_eq!(first_expr(&p.unwrap()), &ExprKind::Number(v), "{src}");
    }
    let (p, _) = build("local x = 1LL");
    assert!(matches!(first_expr(&p.unwrap()), ExprKind::CData(_)));
    // 16 進小数(full-moon の luajit 構文では字句解析されないが、解釈関数としては対応しておく)。
    assert_eq!(
        literal::parse_number("0xA.8p1"),
        Some(literal::NumberLit::Number(21.0))
    );
}

#[test]
fn string_escapes() {
    assert_eq!(literal::unescape(r"a\tb\n\65\x41\\").unwrap(), b"a\tb\nAA\\");
    assert_eq!(literal::unescape("a\\z   \n  b").unwrap(), b"ab");
    assert_eq!(literal::unescape(r"\u{3042}").unwrap(), "あ".as_bytes());
    assert!(literal::unescape(r"\q").is_err());
    assert_eq!(literal::long_string("\nline"), b"line");
    let (p, _) = build("local s = [[\nhello]]");
    assert_eq!(first_expr(&p.unwrap()), &ExprKind::Str(b"hello".to_vec()));
}

#[test]
fn syntax_errors_are_reported_with_positions() {
    let (p, d) = build("local x = \nlocal y = 2 +");
    assert!(p.is_none());
    let e = d.errors().next().expect("error");
    assert_eq!(e.code, "syntax");
    assert!(e.location.line >= 1);
}

#[test]
fn rejects_lua53_operators() {
    // `//` は LuaJIT には無い(Lua 5.3 の整数除算)。
    let (p, d) = build("local x = 1 // 2");
    assert!(p.is_none() && d.has_errors());
}

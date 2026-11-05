// フロントエンド: 独自関数(spec)の呼び出し箇所の静的解決と、誤った使い方の診断。

mod support;

use lua_gpu::diag::Diagnostics;
use lua_gpu::frontend;
use lua_gpu::hir::*;

fn build(src: &str) -> (Option<Program>, Diagnostics) {
    let reg = support::obj_registry();
    let mut diags = Diagnostics::new();
    let fe = frontend::build(src, "test.lua", None, &reg, &mut diags);
    (fe.map(|f| f.program), diags)
}

fn ok(src: &str) -> Program {
    let (p, d) = build(src);
    assert!(!d.has_errors(), "{d}");
    p.unwrap()
}

fn error_codes(src: &str) -> Vec<&'static str> {
    let (_, d) = build(src);
    d.errors().map(|e| e.code).collect()
}

#[test]
fn resolves_calls_properties_and_methods() {
    let p = ok("local r, g = obj.getpixel(1, 2)\n\
                local w = obj.w + obj.h\n\
                obj:putpixel(0, 0, 1, 1, 1, 1)\n\
                debug_log('x')");
    let names: Vec<_> = p.sites.iter().map(|s| (s.name.as_str(), s.args.len(), s.is_method, s.property)).collect();
    assert_eq!(
        names,
        vec![
            ("obj.getpixel", 2, false, false),
            ("obj.w", 0, false, true),
            ("obj.h", 0, false, true),
            ("obj.putpixel", 6, true, false),
            ("debug_log", 1, false, false),
        ]
    );
    // CPU 書き換え用のチェーン位置: `obj.getpixel(...)` は prefix + 2 suffix。
    let s = &p.sites[0];
    assert_eq!(s.suffixes_consumed, 2);
    assert_eq!(&p.files[0].source[s.chain_start as usize..s.chain_start as usize + 3], "obj");
    // property は 1 suffix(`.w`)、メソッド呼び出しも 1 suffix(`:putpixel(...)`)。
    assert_eq!(p.sites[1].suffixes_consumed, 1);
    assert_eq!(p.sites[3].suffixes_consumed, 1);
}

#[test]
fn chains_continue_after_spec_results() {
    let p = ok("local t = obj.getpixel(1, 2).r");
    let StmtKind::Local { exprs, .. } = &p.func(p.main_func()).body.stmts[0].kind else { panic!() };
    let ExprKind::Index { obj, .. } = &exprs[0].kind else { panic!() };
    assert!(matches!(obj.kind, ExprKind::Spec(0)));
}

#[test]
fn local_shadowing_disables_resolution() {
    let p = ok("local obj = {}\nobj.getpixel(1, 2)\nlocal debug_log = print\ndebug_log(1)");
    assert!(p.sites.is_empty());
}

#[test]
fn unregistered_fields_of_reserved_tables_are_plain_lua() {
    let p = ok("obj.custom = 1\nprint(obj.custom)");
    assert!(p.sites.is_empty());
}

#[test]
fn py_functions_can_be_used_as_values() {
    let p = ok("local f = debug_log\nf(1)");
    assert!(p.sites.is_empty());
}

#[test]
fn reports_misuse() {
    assert_eq!(error_codes("local f = obj.getpixel"), vec!["spec-not-called"]);
    assert_eq!(error_codes("obj.w = 3"), vec!["spec-reassign"]);
    assert_eq!(error_codes("obj.getpixel = nil"), vec!["spec-reassign"]);
    assert_eq!(error_codes("function obj.getpixel() end"), vec!["spec-reassign"]);
    assert_eq!(error_codes("obj = {}"), vec!["spec-reserved"]);
    assert_eq!(error_codes("obj:w()"), vec!["spec-not-callable"]);
}

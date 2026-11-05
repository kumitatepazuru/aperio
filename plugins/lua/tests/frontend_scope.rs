// フロントエンド: local / upvalue / global のスコープ解決。

mod support;

use lua_gpu::diag::Diagnostics;
use lua_gpu::frontend;
use lua_gpu::hir::*;

fn build(src: &str) -> Program {
    let reg = support::registry(vec![]);
    let mut diags = Diagnostics::new();
    let fe = frontend::build(src, "test.lua", None, &reg, &mut diags).expect("parse");
    assert!(!diags.has_errors(), "{diags}");
    fe.program
}

fn main_stmts(p: &Program) -> &[Stmt] {
    &p.func(p.main_func()).body.stmts
}

#[test]
fn local_initializer_sees_outer_binding() {
    // `local x = x` の右辺はグローバル x を指す。
    let p = build("local x = x");
    let StmtKind::Local { vars, exprs } = &main_stmts(&p)[0].kind else {
        panic!()
    };
    assert_eq!(p.var(vars[0]).name, "x");
    assert!(matches!(&exprs[0].kind, ExprKind::Global(g) if g == "x"));
}

#[test]
fn shadowing_and_upvalues() {
    let p = build(
        "local a = 1\n\
         local function f() return a end\n\
         do local a = 2; a = 3 end\n\
         b = a",
    );
    let stmts = main_stmts(&p);
    let StmtKind::Local { vars: outer, .. } = &stmts[0].kind else { panic!() };
    let outer_a = outer[0];
    // f の中の a は外側の a(upvalue)。
    let StmtKind::LocalFunction { func, .. } = &stmts[1].kind else { panic!() };
    let StmtKind::Return(r) = &p.func(*func).body.stmts[0].kind else { panic!() };
    assert!(matches!(r[0].kind, ExprKind::Local(v) if v == outer_a));
    assert!(p.var(outer_a).captured);
    // do ブロック内の a は別の変数で、代入されている。
    let StmtKind::Do(b) = &stmts[2].kind else { panic!() };
    let StmtKind::Local { vars: inner, .. } = &b.stmts[0].kind else { panic!() };
    assert_ne!(inner[0], outer_a);
    assert!(p.var(inner[0]).reassigned);
    assert!(!p.var(outer_a).reassigned);
    // 最後の b = a は外側の a、b はグローバル。
    let StmtKind::Assign { targets, exprs } = &stmts[3].kind else { panic!() };
    assert!(matches!(&targets[0], LValue::Global(g, _) if g == "b"));
    assert!(matches!(exprs[0].kind, ExprKind::Local(v) if v == outer_a));
}

#[test]
fn repeat_until_condition_sees_body_locals() {
    let p = build("repeat local done = true until done");
    let StmtKind::Repeat { body, cond } = &main_stmts(&p)[0].kind else { panic!() };
    let StmtKind::Local { vars, .. } = &body.stmts[0].kind else { panic!() };
    assert!(matches!(cond.kind, ExprKind::Local(v) if v == vars[0]));
}

#[test]
fn for_loop_variables_are_scoped_to_the_body() {
    let p = build("for i = 1, 3 do local y = i end\nz = i\nfor k, v in pairs(t) do end");
    let stmts = main_stmts(&p);
    let StmtKind::NumFor { var, body, step, .. } = &stmts[0].kind else { panic!() };
    assert!(step.is_none());
    assert_eq!(p.var(*var).kind, VarKind::ForIndex);
    let StmtKind::Local { exprs, .. } = &body.stmts[0].kind else { panic!() };
    assert!(matches!(exprs[0].kind, ExprKind::Local(v) if v == *var));
    let StmtKind::Assign { exprs, .. } = &stmts[1].kind else { panic!() };
    assert!(matches!(&exprs[0].kind, ExprKind::Global(g) if g == "i"));
    let StmtKind::GenFor { vars, .. } = &stmts[2].kind else { panic!() };
    assert_eq!(vars.len(), 2);
    assert_eq!(p.var(vars[1]).kind, VarKind::ForVar);
}

#[test]
fn function_declarations_become_assignments() {
    let p = build("local M = {}\nfunction M.a.b:c(x, ...) return self, x end\nfunction g() end");
    let stmts = main_stmts(&p);
    let StmtKind::Assign { targets, exprs } = &stmts[1].kind else { panic!() };
    let LValue::Index { key, obj, .. } = &targets[0] else { panic!() };
    assert!(matches!(&key.kind, ExprKind::Str(s) if s == b"c"));
    assert!(matches!(&obj.kind, ExprKind::Index { .. }));
    let ExprKind::Function(f) = exprs[0].kind else { panic!() };
    let f = p.func(f);
    assert_eq!(f.params.len(), 2);
    assert_eq!(p.var(f.params[0]).name, "self");
    assert!(f.is_vararg);
    assert_eq!(f.name.as_deref(), Some("M.a.b:c"));
    let StmtKind::Assign { targets, .. } = &stmts[2].kind else { panic!() };
    assert!(matches!(&targets[0], LValue::Global(g, _) if g == "g"));
}

#[test]
fn spans_point_at_source_positions() {
    let p = build("local a = 1\n  for i = 1, 2 do end");
    let st = &main_stmts(&p)[1];
    assert_eq!(st.span.line, 2);
    assert_eq!(st.span.col, 3);
    assert_eq!(&p.files[0].source[st.span.start as usize..st.span.start as usize + 3], "for");
}

//! 並列化の判定(反復どうしが独立しているか)。
//!
//! 並列次元にするループの本体について:
//! - `break` / `return` / (`goto continue` 以外の)`goto` が無いこと
//! - P の値(GPU でしか得られない値)を外側の変数に代入する場合は、私有化できること
//!   (その変数が本体の外で読まれず、各反復で読む前に必ず代入されている)
//! - 外側のテーブルへ P の値・キー・制御で書き込まないこと
//!
//! U / I の値での外側の変数・テーブルの更新は、CPU の再生で元の順序どおりに行うので問題ない。

use std::collections::HashSet;

use crate::analysis::resolve::walk_expr_children;
use crate::hir::*;
use crate::kernel::bt::{Bt, BtResult};
use crate::kernel::VarRef;

#[derive(Clone, Debug)]
pub enum Blocker {
    /// break / return / goto が本体にある。
    Escape(Span, &'static str),
    /// P の値を、私有化できない外側の変数に代入している。
    CarriedGpuValue(VarRef),
    /// P の値を共有テーブルに書き込んでいる。
    GpuValueToSharedTable(Span),
}

impl Blocker {
    pub fn message(&self, program: &Program) -> String {
        match self {
            Blocker::Escape(_, what) => format!("`{what}` leaves the loop early"),
            Blocker::CarriedGpuValue(v) => format!(
                "variable `{}` receives a GPU-only value and is read outside the loop or before \
                 it is assigned in an iteration",
                var_name(program, v)
            ),
            Blocker::GpuValueToSharedTable(_) => {
                "a GPU-only value is written to a table created outside the loop".to_string()
            }
        }
    }
}

pub fn var_name(program: &Program, v: &VarRef) -> String {
    match v {
        VarRef::Local(id) => program.var(*id).name.clone(),
        VarRef::Global(g) => g.clone(),
    }
}

/// 並列化を妨げるものを返す。空なら並列化できる。私有化する変数を `privatized` に入れる。
pub fn check(
    program: &Program,
    body: &Block,
    body_span: Span,
    bt: &BtResult,
    privatized: &mut HashSet<VarRef>,
) -> Vec<Blocker> {
    let mut blockers = Vec::new();
    let mut labels = HashSet::new();
    collect_labels(body, &mut labels);
    find_escapes(body, 0, &labels, &mut blockers);

    for (var, b) in &bt.outer_assigned {
        if *b < Bt::P {
            continue;
        }
        let ok = match var {
            VarRef::Local(v) => {
                !read_outside(program, *v, body_span) && !upward_exposed(program, body, *v)
            }
            // グローバルはほかの関数からも読めるので私有化しない。
            VarRef::Global(_) => false,
        };
        if ok {
            privatized.insert(var.clone());
        } else {
            blockers.push(Blocker::CarriedGpuValue(var.clone()));
        }
    }

    for w in &bt.table_writes {
        if w.bt == Bt::P {
            blockers.push(Blocker::GpuValueToSharedTable(w.span));
        }
    }
    blockers
}

/// 本体から抜け出す制御(break は内側のループの中なら問題ない。goto は本体内のラベルへなら問題ない)。
fn find_escapes(b: &Block, loop_depth: u32, labels: &HashSet<String>, out: &mut Vec<Blocker>) {
    for s in &b.stmts {
        match &s.kind {
            StmtKind::Break if loop_depth == 0 => out.push(Blocker::Escape(s.span, "break")),
            StmtKind::Return(_) => out.push(Blocker::Escape(s.span, "return")),
            StmtKind::Goto(label) if !labels.contains(label) => {
                out.push(Blocker::Escape(s.span, "goto"))
            }
            StmtKind::Do(inner) => find_escapes(inner, loop_depth, labels, out),
            StmtKind::If { arms, else_block } => {
                for (_, blk) in arms {
                    find_escapes(blk, loop_depth, labels, out);
                }
                if let Some(blk) = else_block {
                    find_escapes(blk, loop_depth, labels, out);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::Repeat { body, .. }
            | StmtKind::NumFor { body, .. }
            | StmtKind::GenFor { body, .. } => find_escapes(body, loop_depth + 1, labels, out),
            _ => {}
        }
    }
}

pub fn collect_labels(b: &Block, out: &mut HashSet<String>) {
    for s in &b.stmts {
        match &s.kind {
            StmtKind::Label(l) => {
                out.insert(l.clone());
            }
            StmtKind::Do(inner) => collect_labels(inner, out),
            StmtKind::If { arms, else_block } => {
                for (_, blk) in arms {
                    collect_labels(blk, out);
                }
                if let Some(blk) = else_block {
                    collect_labels(blk, out);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::Repeat { body, .. }
            | StmtKind::NumFor { body, .. }
            | StmtKind::GenFor { body, .. } => collect_labels(body, out),
            _ => {}
        }
    }
}

/// 変数がカーネル本体の外で読まれているか。
fn read_outside(program: &Program, v: VarId, body: Span) -> bool {
    let mut found = false;
    for f in &program.funcs {
        visit_reads(program, &f.body, &mut |e| {
            if let ExprKind::Local(x) = e.kind {
                if x == v
                    && !(e.span.file == body.file
                        && e.span.start >= body.start
                        && e.span.end <= body.end)
                {
                    found = true;
                }
            }
        });
    }
    found
}

/// 本体の中で、代入より前に読まれることがあるか(反復の初めに前の反復の値が見えてしまう)。
fn upward_exposed(program: &Program, body: &Block, v: VarId) -> bool {
    let mut exposed = false;
    let assigned = scan_block(program, body, v, false, &mut exposed);
    let _ = assigned;
    exposed
}

/// ブロックを順に見て、`v` が確実に代入済みになったかを返す。代入前の読み取りがあれば `exposed`。
fn scan_block(program: &Program, b: &Block, v: VarId, mut assigned: bool, exposed: &mut bool) -> bool {
    for s in &b.stmts {
        assigned = scan_stmt(program, s, v, assigned, exposed);
    }
    assigned
}

fn reads(program: &Program, e: &Expr, v: VarId) -> bool {
    let mut found = false;
    visit_expr(program, e, &mut |x| {
        if matches!(x.kind, ExprKind::Local(id) if id == v) {
            found = true;
        }
    });
    found
}

fn scan_stmt(program: &Program, s: &Stmt, v: VarId, assigned: bool, exposed: &mut bool) -> bool {
    let mut found = false;
    let r = scan_stmt_inner(program, s, v, assigned, &mut found);
    if found {
        *exposed = true;
    }
    r
}

fn scan_stmt_inner(program: &Program, s: &Stmt, v: VarId, assigned: bool, exposed: &mut bool) -> bool {
    let check = |e: &Expr, assigned: bool, exposed: &mut bool| {
        if !assigned && reads(program, e, v) {
            *exposed = true;
        }
    };
    match &s.kind {
        StmtKind::Local { exprs, .. } | StmtKind::Return(exprs) => {
            exprs.iter().for_each(|e| check(e, assigned, exposed));
            assigned
        }
        StmtKind::Assign { targets, exprs } => {
            exprs.iter().for_each(|e| check(e, assigned, exposed));
            let mut now = assigned;
            for t in targets {
                match t {
                    LValue::Local(x, _) if *x == v => now = true,
                    LValue::Index { obj, key, .. } => {
                        check(obj, assigned, exposed);
                        check(key, assigned, exposed);
                    }
                    _ => {}
                }
            }
            now
        }
        StmtKind::Call(e) => {
            check(e, assigned, exposed);
            assigned
        }
        StmtKind::Do(b) => scan_block(program, b, v, assigned, exposed),
        StmtKind::If { arms, else_block } => {
            let mut all = else_block.is_some();
            for (c, b) in arms {
                check(c, assigned, exposed);
                all &= scan_block(program, b, v, assigned, exposed);
            }
            if let Some(b) = else_block {
                all &= scan_block(program, b, v, assigned, exposed);
            }
            assigned || all
        }
        StmtKind::While { cond, body } => {
            check(cond, assigned, exposed);
            scan_block(program, body, v, assigned, exposed);
            assigned
        }
        StmtKind::Repeat { body, cond } => {
            let after = scan_block(program, body, v, assigned, exposed);
            check(cond, after, exposed);
            after
        }
        StmtKind::NumFor {
            start,
            limit,
            step,
            body,
            ..
        } => {
            check(start, assigned, exposed);
            check(limit, assigned, exposed);
            if let Some(st) = step {
                check(st, assigned, exposed);
            }
            scan_block(program, body, v, assigned, exposed);
            assigned
        }
        StmtKind::GenFor { exprs, body, .. } => {
            exprs.iter().for_each(|e| check(e, assigned, exposed));
            scan_block(program, body, v, assigned, exposed);
            assigned
        }
        StmtKind::LocalFunction { func, .. } => {
            // 関数の中から読むなら、呼ばれる時点で代入済みとは限らない。
            if !assigned {
                visit_reads(program, &program.func(*func).body, &mut |e| {
                    if matches!(e.kind, ExprKind::Local(id) if id == v) {
                        *exposed = true;
                    }
                });
            }
            assigned
        }
        StmtKind::Break | StmtKind::Goto(_) | StmtKind::Label(_) => assigned,
    }
}

/// 式とその部分式(関数式の本体も含む)を訪問する。
pub fn visit_expr(program: &Program, e: &Expr, f: &mut impl FnMut(&Expr)) {
    f(e);
    if let ExprKind::Function(func) = e.kind {
        visit_reads(program, &program.func(func).body, f);
    }
    walk_expr_children(program, e, &mut |c| visit_expr(program, c, f));
}

/// ブロック内のすべての式を訪問する。
pub fn visit_reads(program: &Program, b: &Block, f: &mut impl FnMut(&Expr)) {
    for s in &b.stmts {
        match &s.kind {
            StmtKind::Local { exprs, .. } | StmtKind::Return(exprs) => {
                exprs.iter().for_each(|e| visit_expr(program, e, f))
            }
            StmtKind::Assign { targets, exprs } => {
                for t in targets {
                    if let LValue::Index { obj, key, .. } = t {
                        visit_expr(program, obj, f);
                        visit_expr(program, key, f);
                    }
                }
                exprs.iter().for_each(|e| visit_expr(program, e, f));
            }
            StmtKind::Call(e) => visit_expr(program, e, f),
            StmtKind::Do(b) => visit_reads(program, b, f),
            StmtKind::While { cond, body } | StmtKind::Repeat { body, cond } => {
                visit_expr(program, cond, f);
                visit_reads(program, body, f);
            }
            StmtKind::If { arms, else_block } => {
                for (c, b) in arms {
                    visit_expr(program, c, f);
                    visit_reads(program, b, f);
                }
                if let Some(b) = else_block {
                    visit_reads(program, b, f);
                }
            }
            StmtKind::NumFor {
                start,
                limit,
                step,
                body,
                ..
            } => {
                visit_expr(program, start, f);
                visit_expr(program, limit, f);
                if let Some(st) = step {
                    visit_expr(program, st, f);
                }
                visit_reads(program, body, f);
            }
            StmtKind::GenFor { exprs, body, .. } => {
                exprs.iter().for_each(|e| visit_expr(program, e, f));
                visit_reads(program, body, f);
            }
            StmtKind::LocalFunction { func, .. } => visit_reads(program, &program.func(*func).body, f),
            StmtKind::Break | StmtKind::Goto(_) | StmtKind::Label(_) => {}
        }
    }
}

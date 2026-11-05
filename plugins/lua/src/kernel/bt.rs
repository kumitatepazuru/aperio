//! カーネル本体の binding time 解析。
//!
//! - `U`: 反復に依存しない(カーネルの外の値・定数だけで決まる)
//! - `I`: 反復(ループ変数)に依存するが、CPU でも計算できる
//! - `P`: GPU でしか得られない値(GpuFunction の戻り値)に依存する
//!
//! 代入は制御依存も含めて伝播する(P の条件の内側で代入された変数は P)。ループによる持ち越しは
//! 本体全体を不動点まで繰り返して扱う。

use std::collections::{HashMap, HashSet};

use crate::analysis::resolve::{walk_expr_children, Resolver, StaticValue};
use crate::expand::SiteTarget;
use crate::hir::*;
use crate::kernel::VarRef;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Bt {
    U,
    I,
    P,
}

/// 外側のテーブルへの書き込み。
#[derive(Clone, Debug)]
pub struct TableWrite {
    /// 書き込み先テーブルの根の変数(分からなければ None)。
    pub root: Option<VarRef>,
    /// キー・値・制御の binding time の最大。
    pub bt: Bt,
    pub span: Span,
}

#[derive(Clone, Debug, Default)]
pub struct BtResult {
    pub var_bt: HashMap<VarRef, Bt>,
    /// 本体で宣言された local。
    pub body_locals: HashSet<VarId>,
    /// 本体で代入される外側の変数と、その binding time(制御依存を含む)。
    pub outer_assigned: HashMap<VarRef, Bt>,
    /// 外側のテーブルへの書き込み。
    pub table_writes: Vec<TableWrite>,
}

pub struct BtCx<'a> {
    pub program: &'a Program,
    pub resolver: &'a Resolver<'a>,
    pub targets: &'a [SiteTarget],
    pub dim_vars: HashSet<VarId>,
    /// 本体(GpuFunction の呼び出し)を含むユーザー関数。呼び出し結果は P。
    pub gpu_funcs: &'a HashSet<FuncId>,
    pub result: BtResult,
}

impl<'a> BtCx<'a> {
    pub fn new(
        program: &'a Program,
        resolver: &'a Resolver<'a>,
        targets: &'a [SiteTarget],
        dim_vars: HashSet<VarId>,
        gpu_funcs: &'a HashSet<FuncId>,
    ) -> Self {
        Self {
            program,
            resolver,
            targets,
            dim_vars,
            gpu_funcs,
            result: BtResult::default(),
        }
    }

    /// 本体を不動点まで解析する。
    pub fn run(mut self, body: &Block) -> BtResult {
        for _ in 0..64 {
            let before = (
                self.result.var_bt.clone(),
                self.result.outer_assigned.clone(),
            );
            self.result.table_writes.clear();
            self.block(body, Bt::U);
            if before == (self.result.var_bt.clone(), self.result.outer_assigned.clone()) {
                break;
            }
        }
        self.result
    }

    pub fn var(&self, r: &VarRef) -> Bt {
        if let VarRef::Local(v) = r {
            if self.dim_vars.contains(v) {
                return Bt::I;
            }
        }
        self.result.var_bt.get(r).copied().unwrap_or(Bt::U)
    }

    fn raise(&mut self, r: VarRef, bt: Bt) {
        let e = self.result.var_bt.entry(r).or_insert(Bt::U);
        *e = (*e).max(bt);
    }

    pub fn expr(&self, e: &Expr) -> Bt {
        let own = match &e.kind {
            ExprKind::Local(v) => self.var(&VarRef::Local(*v)),
            ExprKind::Global(g) => self.var(&VarRef::Global(g.clone())),
            ExprKind::Spec(site) => match self.targets[*site as usize] {
                SiteTarget::Gpu => Bt::P,
                _ => Bt::U,
            },
            ExprKind::Call { .. } | ExprKind::Method { .. } => match self.resolver.callee(e) {
                Some(StaticValue::Func(f)) if self.gpu_funcs.contains(&f) => Bt::P,
                _ => Bt::U,
            },
            _ => Bt::U,
        };
        let mut bt = own;
        walk_expr_children(self.program, e, &mut |c| bt = bt.max(self.expr(c)));
        bt
    }

    /// 多値を展開した後の i 番目の値の binding time。
    fn value_bt(&self, exprs: &[Expr], i: usize) -> Bt {
        match exprs.get(i) {
            Some(e) => self.expr(e),
            None => match exprs.last() {
                Some(last) if last.is_multi(self.program) => self.expr(last),
                _ => Bt::U,
            },
        }
    }

    fn block(&mut self, b: &Block, ctl: Bt) {
        for s in &b.stmts {
            self.stmt(s, ctl);
        }
    }

    fn stmt(&mut self, s: &Stmt, ctl: Bt) {
        match &s.kind {
            StmtKind::Local { vars, exprs } => {
                for (i, v) in vars.iter().enumerate() {
                    self.result.body_locals.insert(*v);
                    let bt = self.value_bt(exprs, i).max(ctl);
                    self.raise(VarRef::Local(*v), bt);
                }
            }
            StmtKind::LocalFunction { var, .. } => {
                self.result.body_locals.insert(*var);
                self.raise(VarRef::Local(*var), ctl);
            }
            StmtKind::Assign { targets, exprs } => {
                for (i, t) in targets.iter().enumerate() {
                    let val = self.value_bt(exprs, i).max(ctl);
                    match t {
                        LValue::Local(v, _) => {
                            self.raise(VarRef::Local(*v), val);
                            if !self.result.body_locals.contains(v) {
                                let e = self
                                    .result
                                    .outer_assigned
                                    .entry(VarRef::Local(*v))
                                    .or_insert(Bt::U);
                                *e = (*e).max(val);
                            }
                        }
                        LValue::Global(g, _) => {
                            let r = VarRef::Global(g.clone());
                            self.raise(r.clone(), val);
                            let e = self.result.outer_assigned.entry(r).or_insert(Bt::U);
                            *e = (*e).max(val);
                        }
                        LValue::Index { obj, key, span } => {
                            let bt = val.max(self.expr(obj)).max(self.expr(key));
                            let root = table_root(obj);
                            match &root {
                                Some(VarRef::Local(v)) if self.result.body_locals.contains(v) => {
                                    // 本体で作ったテーブル: 中身の binding time をテーブル変数に持たせる。
                                    self.raise(VarRef::Local(*v), bt);
                                }
                                _ => self.result.table_writes.push(TableWrite {
                                    root,
                                    bt,
                                    span: *span,
                                }),
                            }
                        }
                    }
                }
            }
            StmtKind::Call(_) | StmtKind::Return(_) | StmtKind::Break => {}
            StmtKind::Goto(_) | StmtKind::Label(_) => {}
            StmtKind::Do(b) => self.block(b, ctl),
            StmtKind::While { cond, body } => {
                let c = ctl.max(self.expr(cond));
                self.block(body, c);
                // 本体での代入が条件に影響するので、もう一度条件を見る(不動点は run が保証する)。
                let c2 = ctl.max(self.expr(cond));
                if c2 != c {
                    self.block(body, c2);
                }
            }
            StmtKind::Repeat { body, cond } => {
                // 本体は少なくとも 1 回実行され、2 回目以降は条件に制御される。
                self.block(body, ctl);
                let c = ctl.max(self.expr(cond));
                self.block(body, c);
            }
            StmtKind::If { arms, else_block } => {
                let mut c = ctl;
                for (cond, b) in arms {
                    c = c.max(self.expr(cond));
                    self.block(b, c);
                }
                if let Some(b) = else_block {
                    self.block(b, c);
                }
            }
            StmtKind::NumFor {
                var,
                start,
                limit,
                step,
                body,
            } => {
                let mut b = self.expr(start).max(self.expr(limit));
                if let Some(st) = step {
                    b = b.max(self.expr(st));
                }
                let c = ctl.max(b);
                self.result.body_locals.insert(*var);
                // 内側の(逐次の)ループ変数は反復回数に依存するので、少なくとも I。
                self.raise(VarRef::Local(*var), c.max(Bt::I));
                self.block(body, c);
            }
            StmtKind::GenFor { vars, exprs, body } => {
                let b = exprs.iter().fold(Bt::U, |a, e| a.max(self.expr(e)));
                let c = ctl.max(b);
                for v in vars {
                    self.result.body_locals.insert(*v);
                    self.raise(VarRef::Local(*v), c.max(Bt::I));
                }
                self.block(body, c);
            }
        }
    }
}

/// `a.b[c].d` のような添字チェーンの根の変数。
pub fn table_root(e: &Expr) -> Option<VarRef> {
    match &e.kind {
        ExprKind::Local(v) => Some(VarRef::Local(*v)),
        ExprKind::Global(g) => Some(VarRef::Global(g.clone())),
        ExprKind::Index { obj, .. } => table_root(obj),
        ExprKind::Paren(inner) => table_root(inner),
        _ => None,
    }
}

//! 呼び出しの副作用(効果クラス)の解析。
//!
//! 効果クラスは「カーネル内の文を CPU に持ち出してよいか / 再生が必要か / GPU で計算できるか」の判断に使う。
//! メモリ(テーブル・変数)への書き込みはここでは扱わず、カーネルの依存解析で扱う。

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::analysis::resolve::{walk_expr_children, Resolver, StaticValue};
use crate::hir::*;
use crate::stdlib::{Builtin, Effect};

/// 各 site の実装(expand の結果から作る)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteImpl {
    /// CpuFunction: 生成された Lua 関数。
    CpuFunc(FuncId),
    /// GpuFunction(GPU 上では純粋とみなす)。
    Gpu,
    /// PyFunction(実行時のホスト関数。常に Stream 扱い)。
    Py,
    /// 生成に失敗した、または未生成。
    Unknown,
}

pub struct Effects<'a> {
    pub program: &'a Program,
    pub resolver: &'a Resolver<'a>,
    pub sites: &'a [SiteImpl],
    memo: RefCell<HashMap<FuncId, Effect>>,
    in_progress: RefCell<HashSet<FuncId>>,
}

impl<'a> Effects<'a> {
    pub fn new(program: &'a Program, resolver: &'a Resolver<'a>, sites: &'a [SiteImpl]) -> Self {
        Self {
            program,
            resolver,
            sites,
            memo: RefCell::new(HashMap::new()),
            in_progress: RefCell::new(HashSet::new()),
        }
    }

    /// 式を評価したときの効果(子の式を含む)。
    pub fn expr(&self, e: &Expr) -> Effect {
        let mut eff = match &e.kind {
            ExprKind::Call { .. } | ExprKind::Method { .. } => self.call(e),
            ExprKind::Spec(site) => self.site(*site),
            _ => Effect::Pure,
        };
        walk_expr_children(self.program, e, &mut |c| eff = eff.join(self.expr(c)));
        eff
    }

    /// spec 呼び出しそのものの効果。
    pub fn site(&self, site: SiteId) -> Effect {
        match self.sites.get(site as usize).copied().unwrap_or(SiteImpl::Unknown) {
            SiteImpl::CpuFunc(f) => self.func(f),
            SiteImpl::Gpu => Effect::Pure,
            SiteImpl::Py => Effect::Stream,
            SiteImpl::Unknown => Effect::Opaque,
        }
    }

    /// 呼び出しそのものの効果(引数の評価は含まない)。
    pub fn call(&self, call: &Expr) -> Effect {
        // `io.open(path[, "r"]):read(...)` は新しく開いたハンドルからの読み取りなので ReadOnly。
        if let ExprKind::Method { obj, name, .. } = &call.kind {
            if name == "read" {
                if let Some(StaticValue::Builtin(b)) = self.resolver.callee(obj) {
                    if b.name == "io.open" && self.io_open_is_read(obj) {
                        return Effect::ReadOnly;
                    }
                }
            }
        }
        match self.resolver.callee(call) {
            Some(StaticValue::Builtin(b)) => self.builtin_call(b, call),
            Some(StaticValue::Func(f)) => self.func(f),
            _ => Effect::Opaque,
        }
    }

    fn builtin_call(&self, b: &'static Builtin, call: &Expr) -> Effect {
        let args = call_args(call);
        match b.name {
            "io.open" => {
                if self.io_open_is_read(call) {
                    Effect::ReadOnly
                } else {
                    Effect::Write
                }
            }
            // 時刻を引数で与えた変換は純粋。
            "os.time" if !args.is_empty() => Effect::Pure,
            "os.date" if args.len() >= 2 => Effect::Pure,
            _ => b.effect,
        }
    }

    fn io_open_is_read(&self, call: &Expr) -> bool {
        match call_args(call).get(1).map(|e| &e.kind) {
            None => true,
            Some(ExprKind::Str(m)) => matches!(m.as_slice(), b"r" | b"rb"),
            _ => false,
        }
    }

    /// ユーザー関数(本体)の効果。再帰は Opaque。
    pub fn func(&self, f: FuncId) -> Effect {
        if let Some(e) = self.memo.borrow().get(&f) {
            return *e;
        }
        if !self.in_progress.borrow_mut().insert(f) {
            return Effect::Opaque;
        }
        let eff = self.block(&self.program.func(f).body);
        self.in_progress.borrow_mut().remove(&f);
        self.memo.borrow_mut().insert(f, eff);
        eff
    }

    pub fn block(&self, b: &Block) -> Effect {
        b.stmts.iter().fold(Effect::Pure, |acc, s| acc.join(self.stmt(s)))
    }

    fn exprs<'e>(&self, es: impl IntoIterator<Item = &'e Expr>) -> Effect {
        es.into_iter()
            .fold(Effect::Pure, |acc, e| acc.join(self.expr(e)))
    }

    pub fn stmt(&self, s: &Stmt) -> Effect {
        match &s.kind {
            StmtKind::Local { exprs, .. } | StmtKind::Return(exprs) => self.exprs(exprs),
            StmtKind::Assign { targets, exprs } => {
                let mut eff = self.exprs(exprs);
                for t in targets {
                    if let LValue::Index { obj, key, .. } = t {
                        eff = eff.join(self.exprs([obj, key]));
                    }
                }
                eff
            }
            StmtKind::Call(e) => self.expr(e),
            StmtKind::Do(b) => self.block(b),
            StmtKind::While { cond, body } | StmtKind::Repeat { body, cond } => {
                self.expr(cond).join(self.block(body))
            }
            StmtKind::If { arms, else_block } => {
                let mut eff = Effect::Pure;
                for (c, b) in arms {
                    eff = eff.join(self.expr(c)).join(self.block(b));
                }
                if let Some(b) = else_block {
                    eff = eff.join(self.block(b));
                }
                eff
            }
            StmtKind::NumFor {
                start,
                limit,
                step,
                body,
                ..
            } => self
                .exprs([start, limit].into_iter().chain(step.as_ref()))
                .join(self.block(body)),
            StmtKind::GenFor { exprs, body, .. } => {
                let mut eff = self.exprs(exprs).join(self.block(body));
                // 汎用 for のイテレータ呼び出し(pairs / ipairs 以外は中身が分からない)。
                let iter_known = exprs.first().is_some_and(|e| {
                    matches!(
                        self.resolver.callee(e),
                        Some(StaticValue::Builtin(b)) if matches!(b.name, "pairs" | "ipairs")
                    )
                });
                if !iter_known {
                    eff = eff.join(Effect::Opaque);
                }
                eff
            }
            StmtKind::LocalFunction { .. }
            | StmtKind::Break
            | StmtKind::Goto(_)
            | StmtKind::Label(_) => Effect::Pure,
        }
    }
}

/// 呼び出し式の引数(メソッド呼び出しは self を含まない)。
pub fn call_args(call: &Expr) -> &[Expr] {
    match &call.kind {
        ExprKind::Call { args, .. } | ExprKind::Method { args, .. } => args,
        _ => &[],
    }
}

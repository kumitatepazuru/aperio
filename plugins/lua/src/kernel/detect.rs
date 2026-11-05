//! カーネル候補の検出と並列次元の決定。
//!
//! - GpuFunction の呼び出し(直接、またはそれを含むユーザー関数の呼び出し)を含む数値 for を、
//!   並列次元(最大 2 次元の完全な入れ子)にできるか試す。
//! - 並列化できないループは CPU に残し、その中を再帰的に探す。
//! - ループに入っていない GpuFunction の呼び出しは、その GPU の値を使う後続の文とまとめて
//!   単一スレッドのカーネルにする。
//! - カーネルを探すのは、CPU から呼ばれ得る関数(メインチャンク・モジュール、CPU のコードから
//!   参照される関数)だけ。カーネル内からしか呼ばれない関数は GPU 側の関数として変換される。

use std::collections::{HashMap, HashSet};

use crate::analysis::resolve::{walk_expr_children, Resolver, StaticValue};
use crate::diag::Diagnostics;
use crate::expand::SiteTarget;
use crate::hir::*;
use crate::kernel::bt::{BtCx, BtResult, Bt};
use crate::kernel::deps;
use crate::kernel::{Dim, VarRef};

/// 並列化(またはグループ化)できたカーネル候補。
#[derive(Clone, Debug)]
pub struct Candidate {
    pub func: FuncId,
    pub stmts: Vec<StmtId>,
    pub span: Span,
    pub dims: Vec<Dim>,
    pub body: Block,
    pub bt: BtResult,
    pub privatized: HashSet<VarRef>,
}

pub struct Detector<'a> {
    pub program: &'a Program,
    pub resolver: &'a Resolver<'a>,
    pub targets: &'a [SiteTarget],
    pub gpu_funcs: HashSet<FuncId>,
    pub diags: &'a mut Diagnostics,
    pub candidates: Vec<Candidate>,
    /// CPU から参照される関数(カーネルを探す対象)。
    cpu_reachable: HashSet<FuncId>,
    worklist: Vec<FuncId>,
}

/// GpuFunction の呼び出しを(推移的に)含むユーザー関数。
pub fn gpu_funcs(program: &Program, resolver: &Resolver, targets: &[SiteTarget]) -> HashSet<FuncId> {
    let mut direct_calls: HashMap<FuncId, Vec<FuncId>> = HashMap::new();
    let mut set = HashSet::new();
    for f in &program.funcs {
        let mut has_gpu = false;
        let mut calls = Vec::new();
        visit_block_shallow(program, &f.body, &mut |e| match &e.kind {
            ExprKind::Spec(s) if targets[*s as usize] == SiteTarget::Gpu => has_gpu = true,
            ExprKind::Call { .. } | ExprKind::Method { .. } => {
                if let Some(StaticValue::Func(g)) = resolver.callee(e) {
                    calls.push(g);
                }
            }
            _ => {}
        });
        if has_gpu {
            set.insert(f.id);
        }
        direct_calls.insert(f.id, calls);
    }
    loop {
        let mut changed = false;
        for (f, calls) in &direct_calls {
            if !set.contains(f) && calls.iter().any(|g| set.contains(g)) {
                set.insert(*f);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    set
}

impl<'a> Detector<'a> {
    pub fn new(
        program: &'a Program,
        resolver: &'a Resolver<'a>,
        targets: &'a [SiteTarget],
        diags: &'a mut Diagnostics,
    ) -> Self {
        let gpu_funcs = gpu_funcs(program, resolver, targets);
        Self {
            program,
            resolver,
            targets,
            gpu_funcs,
            diags,
            candidates: Vec::new(),
            cpu_reachable: HashSet::new(),
            worklist: Vec::new(),
        }
    }

    pub fn run(mut self) -> (Vec<Candidate>, HashSet<FuncId>) {
        for file in &self.program.files {
            if file.kind != SourceKind::Generated {
                self.reach(file.main);
            }
        }
        while let Some(f) = self.worklist.pop() {
            let body = self.program.func(f).body.clone();
            self.scan_block(f, &body);
        }
        (self.candidates, self.gpu_funcs)
    }

    fn reach(&mut self, f: FuncId) {
        if self.cpu_reachable.insert(f) {
            self.worklist.push(f);
        }
    }

    /// CPU で実行される文から参照される関数を到達可能にする。
    fn mark_refs_stmt(&mut self, s: &Stmt) {
        let program = self.program;
        let mut refs = Vec::new();
        let resolver = self.resolver;
        // `local f = function() end` / `function f() end` の右辺そのものは定義であって参照ではない
        // (呼び出しは解決して拾う)。それ以外の位置の関数式は、どこで呼ばれるか分からないので参照扱い。
        let definition_values: HashSet<FuncId> = match &s.kind {
            StmtKind::Local { exprs, .. } | StmtKind::Assign { exprs, .. } => exprs
                .iter()
                .filter_map(|e| match e.kind {
                    ExprKind::Function(f) => Some(f),
                    _ => None,
                })
                .collect(),
            _ => HashSet::new(),
        };
        visit_stmt_shallow(program, s, &mut |e| match &e.kind {
            ExprKind::Call { .. } | ExprKind::Method { .. } => {
                if let Some(StaticValue::Func(g)) = resolver.callee(e) {
                    refs.push(g);
                }
            }
            ExprKind::Function(g) if !definition_values.contains(g) => refs.push(*g),
            // 名前で参照される関数が呼び出し以外で使われた場合(値として渡す)も参照扱い。
            ExprKind::Local(_) | ExprKind::Global(_) | ExprKind::Index { .. } => {
                if let Some(StaticValue::Func(g)) = resolver.value_of(e) {
                    refs.push(g);
                }
            }
            _ => {}
        });
        for g in refs {
            self.reach(g);
        }
    }

    fn contains_gpu(&self, s: &Stmt) -> bool {
        let mut found = false;
        visit_stmt_shallow(self.program, s, &mut |e| match &e.kind {
            ExprKind::Spec(site) if self.targets[*site as usize] == SiteTarget::Gpu => found = true,
            ExprKind::Call { .. } | ExprKind::Method { .. } => {
                if let Some(StaticValue::Func(g)) = self.resolver.callee(e) {
                    if self.gpu_funcs.contains(&g) {
                        found = true;
                    }
                }
            }
            _ => {}
        });
        found
    }

    fn scan_block(&mut self, func: FuncId, b: &Block) {
        let mut i = 0;
        while i < b.stmts.len() {
            let s = &b.stmts[i];
            if !self.contains_gpu(s) {
                self.mark_refs_stmt(s);
                i += 1;
                continue;
            }
            match &s.kind {
                StmtKind::NumFor { .. } => {
                    if let Some(c) = self.try_parallel(func, s) {
                        self.candidates.push(c);
                    } else {
                        self.mark_header_refs(s);
                        if let StmtKind::NumFor { body, .. } = &s.kind {
                            self.scan_block(func, body);
                        }
                    }
                    i += 1;
                }
                StmtKind::Do(body) => {
                    self.scan_block(func, body);
                    i += 1;
                }
                StmtKind::While { body, .. }
                | StmtKind::Repeat { body, .. }
                | StmtKind::GenFor { body, .. } => {
                    self.mark_header_refs(s);
                    self.scan_block(func, body);
                    i += 1;
                }
                StmtKind::If { arms, else_block } => {
                    self.mark_header_refs(s);
                    for (_, blk) in arms {
                        self.scan_block(func, blk);
                    }
                    if let Some(blk) = else_block {
                        self.scan_block(func, blk);
                    }
                    i += 1;
                }
                _ => {
                    i = self.single_thread_group(func, b, i);
                }
            }
        }
    }

    /// 複合文の条件・ヘッダー部分の参照だけを到達可能にする(本体は scan_block が見る)。
    fn mark_header_refs(&mut self, s: &Stmt) {
        let mut exprs: Vec<&Expr> = Vec::new();
        match &s.kind {
            StmtKind::NumFor {
                start, limit, step, ..
            } => {
                exprs.push(start);
                exprs.push(limit);
                exprs.extend(step.iter());
            }
            StmtKind::While { cond, .. } | StmtKind::Repeat { cond, .. } => exprs.push(cond),
            StmtKind::GenFor { exprs: es, .. } => exprs.extend(es.iter()),
            StmtKind::If { arms, .. } => exprs.extend(arms.iter().map(|(c, _)| c)),
            _ => {}
        }
        let resolver = self.resolver;
        let mut refs = Vec::new();
        for e in exprs {
            visit_expr_shallow(self.program, e, &mut |x| match &x.kind {
                ExprKind::Call { .. } | ExprKind::Method { .. } => {
                    if let Some(StaticValue::Func(g)) = resolver.callee(x) {
                        refs.push(g);
                    }
                }
                ExprKind::Function(g) => refs.push(*g),
                _ => {}
            });
        }
        for g in refs {
            self.reach(g);
        }
    }

    fn dim_of(s: &Stmt) -> Option<Dim> {
        match &s.kind {
            StmtKind::NumFor {
                var,
                start,
                limit,
                step,
                ..
            } => Some(Dim {
                stmt: s.id,
                var: *var,
                start: start.clone(),
                limit: limit.clone(),
                step: step.clone(),
            }),
            _ => None,
        }
    }

    /// ループ本体が(入れ子のループの中ではなく)直接 GPU 関数を呼んでいるか。
    fn calls_gpu_directly(&self, body: &Block) -> bool {
        body.stmts.iter().any(|s| match &s.kind {
            StmtKind::NumFor { .. }
            | StmtKind::GenFor { .. }
            | StmtKind::While { .. }
            | StmtKind::Repeat { .. } => false,
            StmtKind::Do(b) => self.calls_gpu_directly(b),
            StmtKind::If { arms, else_block } => {
                arms.iter().any(|(c, b)| self.expr_has_gpu(c) || self.calls_gpu_directly(b))
                    || else_block.as_ref().is_some_and(|b| self.calls_gpu_directly(b))
            }
            _ => self.contains_gpu(s),
        })
    }

    fn expr_has_gpu(&self, e: &Expr) -> bool {
        self.contains_gpu(&Stmt {
            id: u32::MAX,
            kind: StmtKind::Call(e.clone()),
            span: e.span,
        })
    }

    fn try_parallel(&mut self, func: FuncId, s: &Stmt) -> Option<Candidate> {
        let StmtKind::NumFor { var, body, .. } = &s.kind else {
            return None;
        };
        let outer = Self::dim_of(s)?;
        let mut options: Vec<(Vec<Dim>, Block)> = Vec::new();
        // 並列化するのは、GPU 関数を直接呼ぶ最も内側のループ(またはその完全な 2 重の入れ子)だけ。
        // 外側のループ(例: パスの繰り返し)は、反復どうしが画像を通じて依存し得るので CPU に残す。
        let direct = self.calls_gpu_directly(body);

        // 完全な入れ子で、内側の上下限が外側のループ変数に依存しないなら 2 次元。
        if let [inner] = body.stmts.as_slice() {
            if let StmtKind::NumFor {
                start,
                limit,
                step,
                body: inner_body,
                ..
            } = &inner.kind
            {
                let probe = BtCx::new(
                    self.program,
                    self.resolver,
                    self.targets,
                    [*var].into_iter().collect(),
                    &self.gpu_funcs,
                );
                let bounds = probe
                    .expr(start)
                    .max(probe.expr(limit))
                    .max(step.as_ref().map(|e| probe.expr(e)).unwrap_or(Bt::U));
                if bounds == Bt::U && self.calls_gpu_directly(inner_body) {
                    options.push((
                        vec![outer.clone(), Self::dim_of(inner)?],
                        inner_body.clone(),
                    ));
                }
            }
        }
        if direct {
            options.push((vec![outer], body.clone()));
        }
        if options.is_empty() {
            // GPU 関数は内側のループの中だけ: このループは CPU に残し、中を探す。
            return None;
        }

        let mut last_blockers = Vec::new();
        for (dims, kbody) in options {
            let dim_vars = dims.iter().map(|d| d.var).collect();
            let bt = BtCx::new(self.program, self.resolver, self.targets, dim_vars, &self.gpu_funcs)
                .run(&kbody);
            let body_span = block_span(&kbody, s.span);
            let mut privatized = HashSet::new();
            let blockers = deps::check(self.program, &kbody, body_span, &bt, &mut privatized);
            if blockers.is_empty() {
                return Some(Candidate {
                    func,
                    stmts: vec![s.id],
                    span: s.span,
                    dims,
                    body: kbody,
                    bt,
                    privatized,
                });
            }
            last_blockers = blockers;
        }
        let reasons: Vec<String> = last_blockers.iter().map(|b| b.message(self.program)).collect();
        self.diags.warning(
            "not-parallelized",
            self.program.location(s.span),
            format!(
                "this loop runs on the CPU because it cannot be parallelized: {}",
                reasons.join("; ")
            ),
        );
        None
    }

    /// `b.stmts[start]` から始まる単一スレッドのカーネルを作り、次に見る文の位置を返す。
    fn single_thread_group(&mut self, func: FuncId, b: &Block, start: usize) -> usize {
        let mut end = start + 1;
        loop {
            let group = Block {
                stmts: b.stmts[start..end].to_vec(),
            };
            let bt = BtCx::new(
                self.program,
                self.resolver,
                self.targets,
                HashSet::new(),
                &self.gpu_funcs,
            )
            .run(&group);
            let p_locals: HashSet<VarId> = bt
                .var_bt
                .iter()
                .filter(|(_, b)| **b == Bt::P)
                .filter_map(|(v, _)| match v {
                    VarRef::Local(id) if bt.body_locals.contains(id) => Some(*id),
                    _ => None,
                })
                .collect();
            // 後続の文が GPU の値を読むなら、同じカーネルに含める。
            let next_reads = b.stmts.get(end).is_some_and(|next| {
                let mut found = false;
                deps::visit_reads(
                    self.program,
                    &Block {
                        stmts: vec![next.clone()],
                    },
                    &mut |e| {
                        if matches!(e.kind, ExprKind::Local(v) if p_locals.contains(&v)) {
                            found = true;
                        }
                    },
                );
                found
            });
            if next_reads {
                end += 1;
                continue;
            }
            let span = Span {
                end: b.stmts[end - 1].span.end,
                ..b.stmts[start].span
            };
            for s in &group.stmts {
                if matches!(s.kind, StmtKind::Return(_)) {
                    self.diags.error(
                        "gpu-return",
                        self.program.location(s.span),
                        "values computed on the GPU cannot be returned to CPU code",
                    );
                }
            }
            for (var, b) in &bt.outer_assigned {
                if *b == Bt::P {
                    self.diags.error(
                        "gpu-value-to-cpu",
                        self.program.location(span),
                        format!(
                            "variable `{}` receives a value computed on the GPU but is used by CPU code",
                            deps::var_name(self.program, var)
                        ),
                    );
                }
            }
            for w in &bt.table_writes {
                if w.bt == Bt::P {
                    self.diags.error(
                        "gpu-value-to-cpu",
                        self.program.location(w.span),
                        "a value computed on the GPU cannot be stored into a table used by CPU code",
                    );
                }
            }
            self.diags.warning(
                "single-thread-kernel",
                self.program.location(span),
                "GPU functions outside a parallel loop run in a single GPU thread (slow); \
                 consider calling them inside a loop",
            );
            self.candidates.push(Candidate {
                func,
                stmts: group.stmts.iter().map(|s| s.id).collect(),
                span,
                dims: Vec::new(),
                body: group,
                bt,
                privatized: HashSet::new(),
            });
            return end;
        }
    }
}

/// ブロックが占める範囲(空なら fallback)。
pub fn block_span(b: &Block, fallback: Span) -> Span {
    match (b.stmts.first(), b.stmts.last()) {
        (Some(first), Some(last)) => Span {
            end: last.span.end,
            ..first.span
        },
        _ => fallback,
    }
}

/// 文の中の式を訪問する(入れ子の関数定義の中には入らない)。
pub fn visit_stmt_shallow(program: &Program, s: &Stmt, f: &mut impl FnMut(&Expr)) {
    visit_block_shallow(
        program,
        &Block {
            stmts: vec![s.clone()],
        },
        f,
    )
}

pub fn visit_expr_shallow(program: &Program, e: &Expr, f: &mut impl FnMut(&Expr)) {
    f(e);
    walk_expr_children(program, e, &mut |c| visit_expr_shallow(program, c, f));
}

pub fn visit_block_shallow(program: &Program, b: &Block, f: &mut impl FnMut(&Expr)) {
    for s in &b.stmts {
        match &s.kind {
            StmtKind::Local { exprs, .. } | StmtKind::Return(exprs) => {
                exprs.iter().for_each(|e| visit_expr_shallow(program, e, f))
            }
            StmtKind::Assign { targets, exprs } => {
                for t in targets {
                    if let LValue::Index { obj, key, .. } = t {
                        visit_expr_shallow(program, obj, f);
                        visit_expr_shallow(program, key, f);
                    }
                }
                exprs.iter().for_each(|e| visit_expr_shallow(program, e, f));
            }
            StmtKind::Call(e) => visit_expr_shallow(program, e, f),
            StmtKind::Do(b) => visit_block_shallow(program, b, f),
            StmtKind::While { cond, body } | StmtKind::Repeat { body, cond } => {
                visit_expr_shallow(program, cond, f);
                visit_block_shallow(program, body, f);
            }
            StmtKind::If { arms, else_block } => {
                for (c, b) in arms {
                    visit_expr_shallow(program, c, f);
                    visit_block_shallow(program, b, f);
                }
                if let Some(b) = else_block {
                    visit_block_shallow(program, b, f);
                }
            }
            StmtKind::NumFor {
                start,
                limit,
                step,
                body,
                ..
            } => {
                visit_expr_shallow(program, start, f);
                visit_expr_shallow(program, limit, f);
                if let Some(st) = step {
                    visit_expr_shallow(program, st, f);
                }
                visit_block_shallow(program, body, f);
            }
            StmtKind::GenFor { exprs, body, .. } => {
                exprs.iter().for_each(|e| visit_expr_shallow(program, e, f));
                visit_block_shallow(program, body, f);
            }
            StmtKind::LocalFunction { .. }
            | StmtKind::Break
            | StmtKind::Goto(_)
            | StmtKind::Label(_) => {}
        }
    }
}

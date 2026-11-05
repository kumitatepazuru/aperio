//! カーネル本体の振り分けと GPU 側の型付け。
//!
//! - GPU で計算する文・式を決め、型(GpuTy)を付ける。
//! - GPU コードが必要とする値のうち CPU でしか計算できないもの(文字列処理・乱数・ホスト関数など)を、
//!   ループ不変なら持ち出し(hoist)、そうでなければ CPU の再生で反復ごとに記録(record)する。
//! - CPU の再生が必要な文(副作用・外側の状態の更新・記録の計算に必要な定義)を求める。
//! - GPU でしか得られない値(P)に依存する CPU 専用の処理はエラーにする。

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;

use crate::analysis::effects::{Effects, SiteImpl};
use crate::analysis::resolve::{walk_expr_children, Resolver, StaticValue};
use crate::diag::Diagnostics;
use crate::expand::SiteTarget;
use crate::hir::*;
use crate::kernel::bt::{table_root, Bt, BtCx};
use crate::kernel::detect::Candidate;
use crate::kernel::types::{ArrLen, FuncSig, GpuTy};
use crate::kernel::*;
use crate::stdlib::{Effect, GpuImpl, Kind};

pub struct Classifier<'a> {
    pub program: &'a Program,
    pub resolver: &'a Resolver<'a>,
    pub effects: &'a Effects<'a>,
    pub inputs: &'a Inputs<'a>,
    pub gpu_funcs: &'a HashSet<FuncId>,
    pub diags: &'a mut Diagnostics,
    bt: BtCx<'a>,
    k: Kernel,
    /// 本体で定義される local のうち、CPU でしか値が分からないもの(推定型)。
    cpu_defined: HashMap<VarId, GpuTy>,
    body_locals: HashSet<VarId>,
    dim_vars: HashSet<VarId>,
    /// 本体に Write 以上の効果があるか(このとき ReadOnly の持ち出しをしない)。
    body_has_write: bool,
    /// 現在の制御の binding time。
    ctl: Bt,
    /// P の条件で回るループの中か(記録の個数が分からない)。
    in_p_loop: bool,
    /// 記録を含み得る内側の逐次ループ(record_loops の添字)。
    record_loop: Option<usize>,
    /// 記録ループの外での次の記録位置。
    next_offset: u32,
    /// 関数の GPU 変換中か(関数内では CPU の値は使えない)。
    in_func: Option<FuncId>,
    funcs_in_progress: HashSet<FuncId>,
    pub expr_types: HashMap<ExprKey, GpuTy>,
    /// 再生が必要な文(葉の文)。
    replay_leaves: HashSet<StmtId>,
    /// 記録・持ち出しの式が参照する本体の local。
    replay_needed_vars: HashSet<VarId>,
    /// 記録する式を含む文。
    record_stmts: HashSet<StmtId>,
    /// 再生でも同じ構造で実行する、CPU で判定できる条件付きの飛び越し(goto / break を含む if)。
    replay_controls: HashSet<StmtId>,
    /// いま見ている文(記録を含む文として印を付けるため)。
    cur_stmt: Option<StmtId>,
    /// 記録できない場所(while / repeat / 記録ループの中のループ)なら、その理由。
    record_blocked: Option<&'static str>,
    /// `and` / `or` の右辺(短絡評価で条件付きになる)の中か。
    short_circuit_depth: u32,
    errors: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Placement {
    Gpu,
    Cpu,
}

impl<'a> Classifier<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        program: &'a Program,
        resolver: &'a Resolver<'a>,
        effects: &'a Effects<'a>,
        inputs: &'a Inputs<'a>,
        gpu_funcs: &'a HashSet<FuncId>,
        diags: &'a mut Diagnostics,
        id: u32,
        cand: Candidate,
    ) -> Self {
        let dim_vars: HashSet<VarId> = cand.dims.iter().map(|d| d.var).collect();
        let mut bt = BtCx::new(program, resolver, inputs.targets, dim_vars.clone(), gpu_funcs);
        bt.result = cand.bt.clone();
        let carried: HashSet<VarRef> = cand
            .bt
            .outer_assigned
            .iter()
            .filter(|(v, b)| **b < Bt::P && !cand.privatized.contains(*v))
            .map(|(v, _)| v.clone())
            .collect();
        let written_tables: HashSet<VarRef> = cand
            .bt
            .table_writes
            .iter()
            .filter_map(|w| w.root.clone())
            .collect();
        let body_has_write = effects.block(&cand.body) >= Effect::Write;
        let single_thread = cand.dims.is_empty();
        let k = Kernel {
            id,
            func: cand.func,
            stmts: cand.stmts,
            span: cand.span,
            dims: cand.dims,
            body: cand.body,
            var_bt: cand.bt.var_bt.clone(),
            privatized: cand.privatized,
            carried,
            written_tables,
            captures: IndexMap::new(),
            hoists: Vec::new(),
            records: Vec::new(),
            record_loops: Vec::new(),
            slots_per_iteration: 0,
            needs_replay: false,
            cpu_refs: HashMap::new(),
            var_types: HashMap::new(),
            privatized_globals: HashMap::new(),
            funcs: Vec::new(),
            func_types: HashMap::new(),
            gpu_stmts: HashSet::new(),
            replay_leaves: HashSet::new(),
            record_stmts: HashSet::new(),
            replay_controls: HashSet::new(),
            expr_types: HashMap::new(),
            single_thread,
        };
        Self {
            program,
            resolver,
            effects,
            inputs,
            gpu_funcs,
            diags,
            bt,
            body_locals: cand.bt.body_locals.clone(),
            k,
            cpu_defined: HashMap::new(),
            dim_vars,
            body_has_write,
            ctl: Bt::U,
            in_p_loop: false,
            record_loop: None,
            next_offset: 0,
            in_func: None,
            funcs_in_progress: HashSet::new(),
            expr_types: HashMap::new(),
            replay_leaves: HashSet::new(),
            replay_needed_vars: HashSet::new(),
            record_stmts: HashSet::new(),
            replay_controls: HashSet::new(),
            cur_stmt: None,
            record_blocked: None,
            short_circuit_depth: 0,
            errors: false,
        }
    }

    fn error(&mut self, code: &'static str, span: Span, msg: impl Into<String>) {
        self.errors = true;
        let loc = self.program.location(span);
        self.diags.error(code, loc, msg);
    }

    fn warn(&mut self, code: &'static str, span: Span, msg: impl Into<String>) {
        let loc = self.program.location(span);
        self.diags.warning(code, loc, msg);
    }

    /// 振り分けを行い、カーネルの計画を返す(エラーがあれば None)。
    pub fn run(mut self) -> Option<Kernel> {
        let body = self.k.body.clone();
        self.place_locals(&body);
        self.block(&body);
        self.finish_replay(&body);

        // 記録スロットの数。
        let mut slots = self.next_offset;
        for l in &self.k.record_loops {
            slots = slots.max(l.base + l.stride * l.max_iterations);
        }
        self.k.slots_per_iteration = slots;
        for c in self.k.captures.values_mut() {
            c.ty = c.ty.defaulted();
        }
        for t in self.k.var_types.values_mut() {
            *t = t.defaulted();
        }
        self.k.expr_types = std::mem::take(&mut self.expr_types)
            .into_iter()
            .map(|(k, t)| (k, t.defaulted()))
            .collect();
        if self.errors {
            None
        } else {
            Some(self.k)
        }
    }

    // ---- local の置き場所(GPU / CPU)を決める -----------------------------------

    /// 本体の local のうち、CPU でしか定義できないもの(文字列・CPU 専用の呼び出しの結果など)を求める。
    fn place_locals(&mut self, body: &Block) {
        for _ in 0..16 {
            let before = self.cpu_defined.len();
            self.place_block(body);
            if self.cpu_defined.len() == before {
                break;
            }
        }
    }

    fn place_block(&mut self, b: &Block) {
        for s in &b.stmts {
            match &s.kind {
                StmtKind::Local { vars, exprs } => self.place_assign(
                    &vars.iter().map(|v| Some(*v)).collect::<Vec<_>>(),
                    exprs,
                ),
                StmtKind::Assign { targets, exprs } => {
                    let vars: Vec<Option<VarId>> = targets
                        .iter()
                        .map(|t| match t {
                            LValue::Local(v, _) if self.body_locals.contains(v) => Some(*v),
                            _ => None,
                        })
                        .collect();
                    self.place_assign(&vars, exprs);
                }
                StmtKind::Do(b) => self.place_block(b),
                StmtKind::If { arms, else_block } => {
                    for (_, blk) in arms {
                        self.place_block(blk);
                    }
                    if let Some(blk) = else_block {
                        self.place_block(blk);
                    }
                }
                StmtKind::While { body, .. }
                | StmtKind::Repeat { body, .. }
                | StmtKind::NumFor { body, .. } => self.place_block(body),
                StmtKind::GenFor { vars, body, .. } => {
                    for v in vars {
                        self.cpu_defined.entry(*v).or_insert(GpuTy::Unknown);
                    }
                    self.place_block(body);
                }
                _ => {}
            }
        }
    }

    fn place_assign(&mut self, vars: &[Option<VarId>], exprs: &[Expr]) {
        for (i, v) in vars.iter().enumerate() {
            let Some(v) = v else { continue };
            if self.cpu_defined.contains_key(v) {
                continue;
            }
            let value = match exprs.get(i) {
                Some(e) => Some(e),
                None => exprs.last().filter(|l| l.is_multi(self.program)),
            };
            let Some(e) = value else { continue };
            if self.placement(e) == Placement::Cpu {
                let ty = self.guess_type(e);
                self.cpu_defined.insert(*v, ty);
            }
        }
    }

    /// 式(の値)を GPU で求められるか。CPU の値を部分式として持ち出せる場合も GPU とみなす。
    fn placement(&self, e: &Expr) -> Placement {
        if self.can_gpu(e, 0) {
            Placement::Gpu
        } else {
            Placement::Cpu
        }
    }

    /// 式の演算そのものが GPU で実装でき、部分式も GPU で求めるか CPU の値にできるか。
    fn can_gpu(&self, e: &Expr, depth: u32) -> bool {
        if depth > 64 {
            return false;
        }
        let child_ok = |c: &Expr| self.can_gpu(c, depth + 1) || self.can_be_cpu_value(c);
        match &e.kind {
            ExprKind::Number(_) | ExprKind::Bool(_) => true,
            ExprKind::Nil | ExprKind::Str(_) | ExprKind::CData(_) | ExprKind::Vararg => false,
            ExprKind::Function(_) => false,
            ExprKind::Local(v) => {
                if self.dim_vars.contains(v) {
                    return true;
                }
                if self.body_locals.contains(v) {
                    return !self.cpu_defined.contains_key(v);
                }
                let r = VarRef::Local(*v);
                if self.k.privatized.contains(&r) {
                    return true;
                }
                !self.k.carried.contains(&r) && self.capture_guess(e).is_storable_or_unknown()
            }
            ExprKind::Global(g) => {
                let r = VarRef::Global(g.clone());
                if self.k.carried.contains(&r) {
                    return false;
                }
                matches!(self.resolver.value_of(e), Some(StaticValue::Builtin(_)))
                    || self.capture_guess(e).is_storable_or_unknown()
            }
            ExprKind::Index { obj, key } => {
                if external_name(e).is_some() {
                    return true;
                }
                if let Some(StaticValue::Builtin(b)) = self.resolver.value_of(e) {
                    return matches!(b.gpu, GpuImpl::Const(_)) || b.kind == Kind::Function;
                }
                if let Some(root) = table_root(obj) {
                    if self.k.written_tables.contains(&root) {
                        return false;
                    }
                }
                // `t.name` のキーは静的なフィールド名(文字列の値として GPU に置く必要はない)。
                let key_ok = matches!(key.kind, ExprKind::Str(_)) || child_ok(key);
                self.can_gpu(obj, depth + 1) && key_ok
            }
            ExprKind::Call { args, .. } | ExprKind::Method { args, .. } => {
                let callee_ok = match self.resolver.callee(e) {
                    Some(StaticValue::Builtin(b)) => self.builtin_gpu_ok(b, args),
                    Some(StaticValue::Func(f)) => self.func_gpu_ok(f),
                    _ => false,
                };
                callee_ok && args.iter().all(child_ok)
            }
            ExprKind::Spec(site) => {
                let s = self.program.site(*site);
                let ok = match self.inputs.targets[*site as usize] {
                    SiteTarget::Gpu => true,
                    SiteTarget::Cpu => match self.inputs.site_impls[*site as usize] {
                        SiteImpl::CpuFunc(f) => self.func_gpu_ok(f),
                        _ => false,
                    },
                    SiteTarget::Py => false,
                };
                ok && s.args.iter().all(child_ok)
            }
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::Concat => false,
                BinOp::Eq | BinOp::Ne if is_type_compare(self, lhs, rhs) => true,
                _ => {
                    let lt = self.guess_type(lhs);
                    let rt = self.guess_type(rhs);
                    if matches!(lt, GpuTy::Cpu) || matches!(rt, GpuTy::Cpu) {
                        return false;
                    }
                    child_ok(lhs) && child_ok(rhs)
                }
            },
            ExprKind::Unary { op, expr } => match op {
                UnOp::Len => {
                    !matches!(self.guess_type(expr), GpuTy::Float | GpuTy::Bool | GpuTy::Cpu | GpuTy::Record(_))
                        && self.can_gpu(expr, depth + 1)
                }
                _ => !matches!(self.guess_type(expr), GpuTy::Cpu) && child_ok(expr),
            },
            ExprKind::Paren(inner) => self.can_gpu(inner, depth + 1),
            ExprKind::Table(fields) => fields.iter().all(|f| match f {
                TableField::Positional(v) | TableField::Named(_, v) => child_ok(v),
                TableField::Keyed(..) => false,
            }),
        }
    }

    /// CPU で計算して GPU に渡せる(スカラーの)値か。
    fn can_be_cpu_value(&self, e: &Expr) -> bool {
        self.bt.expr(e) < Bt::P && self.in_func.is_none() && self.guess_type(e).is_scalar_or_unknown()
    }

    fn builtin_gpu_ok(&self, b: &crate::stdlib::Builtin, args: &[Expr]) -> bool {
        match b.gpu {
            GpuImpl::Intrinsic(_) | GpuImpl::Helper(_) => true,
            GpuImpl::Special => match b.name {
                // tonumber は数値に対してだけ恒等(文字列の変換は CPU)。
                "tonumber" => args
                    .first()
                    .is_some_and(|a| matches!(self.guess_type(a), GpuTy::Float)),
                "assert" | "error" | "rawequal" | "select" | "type" => true,
                "rawget" => true,
                "string.len" | "string.byte" => args.first().is_some_and(|a| matches!(a.kind, ExprKind::Str(_))),
                _ => false,
            },
            GpuImpl::Const(_) | GpuImpl::None => false,
        }
    }

    /// ユーザー関数(または CpuFunction の生成関数)を GPU に変換できるか(構造だけで判定)。
    fn func_gpu_ok(&self, f: FuncId) -> bool {
        let func = self.program.func(f);
        if func.is_vararg || self.effects.func(f) > Effect::Pure && !self.gpu_funcs.contains(&f) {
            // GpuFunction を含む関数は GPU でしか実行できないので、効果に関わらず GPU に置く。
            return self.gpu_funcs.contains(&f) && !func.is_vararg;
        }
        !func.is_vararg
    }

    /// 型の推定(構造的。CPU / GPU どちらに置くかの判定と、CPU の値の型に使う)。
    fn guess_type(&self, e: &Expr) -> GpuTy {
        match &e.kind {
            ExprKind::Number(_) => GpuTy::Float,
            ExprKind::Bool(_) => GpuTy::Bool,
            ExprKind::Str(_) | ExprKind::Nil | ExprKind::CData(_) | ExprKind::Vararg => GpuTy::Cpu,
            ExprKind::Function(f) => GpuTy::Func(*f),
            ExprKind::Paren(inner) => self.guess_type(inner),
            ExprKind::Binary { op, lhs, .. } => match op {
                BinOp::Concat => GpuTy::Cpu,
                o if o.is_arith() => GpuTy::Float,
                o if o.is_compare() => GpuTy::Bool,
                _ => self.guess_type(lhs),
            },
            ExprKind::Unary { op, .. } => match op {
                UnOp::Not => GpuTy::Bool,
                _ => GpuTy::Float,
            },
            ExprKind::Table(fields) => {
                if fields.iter().all(|f| matches!(f, TableField::Positional(_))) {
                    let elem = fields
                        .iter()
                        .map(|f| match f {
                            TableField::Positional(v) => self.guess_type(v),
                            _ => GpuTy::Unknown,
                        })
                        .fold(GpuTy::Unknown, |a, t| a.unify(&t).unwrap_or(GpuTy::Cpu));
                    GpuTy::Array(Box::new(elem), ArrLen::Local(fields.len() as u32))
                } else if fields.iter().all(|f| matches!(f, TableField::Named(..))) {
                    GpuTy::Record(
                        fields
                            .iter()
                            .filter_map(|f| match f {
                                TableField::Named(k, v) => Some((k.clone(), self.guess_type(v))),
                                _ => None,
                            })
                            .collect(),
                    )
                } else {
                    GpuTy::Cpu
                }
            }
            ExprKind::Local(v) => {
                if self.dim_vars.contains(v) {
                    return GpuTy::Float;
                }
                if let Some(t) = self.cpu_defined.get(v) {
                    return t.clone();
                }
                if let Some(t) = self.k.var_types.get(v) {
                    return t.clone();
                }
                if self.body_locals.contains(v) {
                    // 宣言時の初期値から推定する(初期値は宣言より前の変数しか参照しないので循環しない)。
                    return match self.local_init(*v) {
                        Some(init) => self.guess_type(&init),
                        None => GpuTy::Unknown,
                    };
                }
                self.capture_guess(e)
            }
            ExprKind::Global(_) => match self.resolver.value_of(e) {
                Some(StaticValue::Builtin(b)) => GpuTy::Builtin(b),
                Some(StaticValue::Func(f)) => GpuTy::Func(f),
                _ => self.capture_guess(e),
            },
            ExprKind::Index { obj, key } => {
                if let Some(name) = external_name(e) {
                    return self
                        .inputs
                        .externals
                        .get(name)
                        .map(GpuTy::from_spec)
                        .unwrap_or(GpuTy::Unknown);
                }
                match self.resolver.value_of(e) {
                    Some(StaticValue::Builtin(b)) => {
                        return match b.gpu {
                            GpuImpl::Const(_) => GpuTy::Float,
                            _ => GpuTy::Builtin(b),
                        }
                    }
                    Some(StaticValue::Func(f)) => return GpuTy::Func(f),
                    _ => {}
                }
                match (self.guess_type(obj), &key.kind) {
                    (GpuTy::Record(fields), ExprKind::Str(k)) => fields
                        .iter()
                        .find(|(n, _)| n.as_bytes() == k.as_slice())
                        .map(|(_, t)| t.clone())
                        .unwrap_or(GpuTy::Unknown),
                    (GpuTy::Array(elem, _), _) => (*elem).clone(),
                    (GpuTy::Cpu, _) => GpuTy::Cpu,
                    _ => GpuTy::Unknown,
                }
            }
            // file:read / file:lines / 文字列メソッドの結果は文字列(CPU 専用)。
            ExprKind::Method { name, .. }
                if matches!(
                    name.as_str(),
                    "read" | "lines" | "format" | "sub" | "upper" | "lower" | "rep" | "gsub" | "match"
                        | "reverse" | "char"
                ) =>
            {
                GpuTy::Cpu
            }
            ExprKind::Call { .. } | ExprKind::Method { .. } => match self.resolver.callee(e) {
                Some(StaticValue::Builtin(b)) => match b.gpu {
                    GpuImpl::Intrinsic(_) | GpuImpl::Helper(_) => GpuTy::Float,
                    _ => match b.name {
                        "tonumber" | "string.len" | "string.byte" | "math.random" | "os.time"
                        | "os.clock" | "select" => GpuTy::Float,
                        "rawequal" => GpuTy::Bool,
                        "assert" => call_arg(e, 0).map(|a| self.guess_type(a)).unwrap_or(GpuTy::Unknown),
                        _ if b.name.starts_with("string.") || b.name == "tostring" || b.name == "type" => {
                            GpuTy::Cpu
                        }
                        _ => GpuTy::Unknown,
                    },
                },
                Some(StaticValue::Func(f)) => self
                    .k
                    .func_types
                    .get(&f)
                    .and_then(|s| s.returns.first().cloned())
                    .unwrap_or(GpuTy::Unknown),
                _ => GpuTy::Unknown,
            },
            ExprKind::Spec(site) => {
                let i = *site as usize;
                match self.inputs.targets[i] {
                    SiteTarget::Gpu => self.inputs.gpu_returns[i]
                        .first()
                        .map(GpuTy::from_spec)
                        .unwrap_or(GpuTy::Cpu),
                    _ => GpuTy::Unknown,
                }
            }
        }
    }

    /// 捕捉する外側の値の型の推定(宣言時の値から)。分からなければ Unknown(使われ方で決める)。
    fn capture_guess(&self, e: &Expr) -> GpuTy {
        match &e.kind {
            ExprKind::Local(_) | ExprKind::Global(_) => {
                if let Some(c) = self.k.captures.get(&capture_key_of(e)) {
                    return c.ty.clone();
                }
                match self.resolver.value_of(e) {
                    Some(StaticValue::Func(f)) => GpuTy::Func(f),
                    Some(StaticValue::Builtin(b)) => GpuTy::Builtin(b),
                    Some(StaticValue::LocalTable(v)) => match self.local_init(v) {
                        Some(init) => self.guess_type(&init),
                        None => GpuTy::Unknown,
                    },
                    _ => match &e.kind {
                        ExprKind::Local(v) => match self.local_init(*v) {
                            Some(init) if !self.program.var(*v).reassigned => {
                                let t = self.guess_type(&init);
                                // 外側の local の初期値が local 自身を参照していることはないので再帰しない。
                                if matches!(t, GpuTy::Cpu) { GpuTy::Cpu } else { GpuTy::Unknown.unify(&t).unwrap_or(GpuTy::Unknown) }
                            }
                            _ => GpuTy::Unknown,
                        },
                        _ => GpuTy::Unknown,
                    },
                }
            }
            _ => GpuTy::Unknown,
        }
    }

    fn local_init(&self, v: VarId) -> Option<Expr> {
        // 宣言文を探す(外側の local の初期値)。
        let info = self.program.var(v);
        let f = self.program.func(info.func);
        find_local_init(&f.body, v)
    }

    // ---- 文 ---------------------------------------------------------------------

    fn block(&mut self, b: &Block) {
        let saved = self.ctl;
        for s in &b.stmts {
            self.stmt(s);
            // 条件付きの goto / break の後ろの文は、その条件に制御依存する。
            if let Some(j) = self.bt.jump_bt(s) {
                if !matches!(s.kind, StmtKind::Break | StmtKind::Goto(_)) {
                    if j < Bt::P && self.in_func.is_none() {
                        // CPU でも判定できる飛び越しは、再生でも同じ構造で行う。
                        self.replay_controls.insert(s.id);
                    }
                    self.ctl = self.ctl.max(j);
                }
            }
        }
        self.ctl = saved;
    }

    fn mark_gpu(&mut self, s: &Stmt) {
        self.k.gpu_stmts.insert(s.id);
    }

    fn stmt(&mut self, s: &Stmt) {
        let saved = self.cur_stmt.replace(s.id);
        self.stmt_inner(s);
        self.cur_stmt = saved;
    }

    fn stmt_inner(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Local { vars, exprs } => {
                let targets: Vec<Target> = vars.iter().map(|v| Target::Var(*v)).collect();
                self.assign(s, &targets, exprs);
            }
            StmtKind::Assign { targets, exprs } => {
                let ts: Vec<Target> = targets
                    .iter()
                    .map(|t| match t {
                        LValue::Local(v, _) => {
                            let r = VarRef::Local(*v);
                            if self.body_locals.contains(v) || self.k.privatized.contains(&r) {
                                Target::Var(*v)
                            } else {
                                Target::Outer
                            }
                        }
                        LValue::Global(..) => Target::Outer,
                        LValue::Index { obj, key, .. } => match table_root(obj) {
                            Some(VarRef::Local(v)) if self.body_locals.contains(&v) => {
                                Target::LocalIndex(obj.clone(), key.clone())
                            }
                            _ => Target::SharedIndex,
                        },
                    })
                    .collect();
                self.assign(s, &ts, exprs);
            }
            StmtKind::Call(e) => self.call_stmt(s, e),
            StmtKind::Do(b) => {
                self.block(b);
                if self.any_gpu(b) {
                    self.mark_gpu(s);
                }
            }
            StmtKind::If { arms, else_block } => {
                let saved = self.ctl;
                let mut c = self.ctl;
                for (cond, blk) in arms {
                    c = c.max(self.bt.expr(cond));
                    self.ctl = c;
                    self.block(blk);
                }
                if let Some(blk) = else_block {
                    self.block(blk);
                }
                self.ctl = saved;
                let gpu = arms.iter().any(|(_, b)| self.any_gpu(b))
                    || else_block.as_ref().is_some_and(|b| self.any_gpu(b));
                if gpu {
                    for (cond, _) in arms {
                        self.gexpr(cond, &GpuTy::Unknown);
                    }
                    self.mark_gpu(s);
                }
            }
            StmtKind::While { cond, body } => self.loop_stmt(s, Some(cond), body, false),
            StmtKind::Repeat { body, cond } => self.loop_stmt(s, Some(cond), body, true),
            StmtKind::NumFor {
                var,
                start,
                limit,
                step,
                body,
            } => {
                let b = [Some(start), Some(limit), step.as_ref()]
                    .into_iter()
                    .flatten()
                    .fold(Bt::U, |a, e| a.max(self.bt.expr(e)));
                let saved_ctl = self.ctl;
                let saved_p = self.in_p_loop;
                let saved_loop = self.record_loop;
                self.ctl = self.ctl.max(b);
                if b == Bt::P {
                    self.in_p_loop = true;
                }
                let saved_blocked = self.record_blocked;
                if self.record_loop.is_some() {
                    self.record_blocked = Some("loops nested inside another inner loop");
                }
                // 記録を含むかもしれないので、記録ループを用意しておく(使われなければ消す)。
                let loop_idx = if b < Bt::P
                    && self.record_loop.is_none()
                    && self.record_blocked.is_none()
                    && self.in_func.is_none()
                {
                    let idx = self.k.record_loops.len();
                    self.k.record_loops.push(RecordLoop {
                        stmt: s.id,
                        stride: 0,
                        base: self.next_offset,
                        max_iterations: self.inputs.options.max_values_per_iteration,
                    });
                    self.record_loop = Some(idx);
                    Some(idx)
                } else {
                    None
                };
                self.k.var_types.insert(*var, GpuTy::Float);
                self.block(body);
                if let Some(idx) = loop_idx {
                    let l = &self.k.record_loops[idx];
                    if l.stride == 0 && idx + 1 == self.k.record_loops.len() {
                        self.k.record_loops.pop();
                    } else {
                        self.next_offset = l.base + l.stride * l.max_iterations;
                    }
                }
                self.record_loop = saved_loop;
                self.record_blocked = saved_blocked;
                self.ctl = saved_ctl;
                self.in_p_loop = saved_p;
                if self.any_gpu(body) {
                    for e in [Some(start), Some(limit), step.as_ref()].into_iter().flatten() {
                        self.gexpr(e, &GpuTy::Float);
                    }
                    self.mark_gpu(s);
                }
            }
            StmtKind::GenFor { exprs, body, .. } => {
                // 汎用 for は CPU でのみ扱う(GPU 側に必要な部分があればエラー)。
                let saved = self.ctl;
                let saved_blocked = self.record_blocked.replace("generic `for ... in` loops");
                self.ctl = self.ctl.max(Bt::I);
                self.block(body);
                self.ctl = saved;
                self.record_blocked = saved_blocked;
                if self.any_gpu(body) {
                    self.error(
                        "gpu-generic-for",
                        s.span,
                        "generic `for ... in` loops cannot contain GPU work; use a numeric for loop",
                    );
                }
                let _ = exprs;
                self.replay_leaves.insert(s.id);
            }
            StmtKind::LocalFunction { .. } => {
                if self.in_func.is_none() {
                    self.replay_leaves.insert(s.id);
                }
            }
            StmtKind::Return(exprs) => {
                if self.in_func.is_some() {
                    for e in exprs {
                        self.gexpr(e, &GpuTy::Unknown);
                    }
                    self.mark_gpu(s);
                }
            }
            // 飛び越し自体は副作用ではない(再生では、含む構造が再生されるときに一緒に出力する)。
            StmtKind::Break | StmtKind::Goto(_) | StmtKind::Label(_) => self.mark_gpu(s),
        }
    }

    fn loop_stmt(&mut self, s: &Stmt, cond: Option<&Expr>, body: &Block, _repeat: bool) {
        let c = cond.map(|e| self.bt.expr(e)).unwrap_or(Bt::U);
        let saved_ctl = self.ctl;
        let saved_p = self.in_p_loop;
        let saved_blocked = self.record_blocked.replace("`while` / `repeat` loops");
        self.ctl = self.ctl.max(c).max(Bt::I);
        if c == Bt::P {
            self.in_p_loop = true;
        }
        self.block(body);
        self.ctl = saved_ctl;
        self.in_p_loop = saved_p;
        self.record_blocked = saved_blocked;
        if self.any_gpu(body) || c == Bt::P {
            if let Some(e) = cond {
                self.gexpr(e, &GpuTy::Unknown);
            }
            self.mark_gpu(s);
        }
    }

    fn any_gpu(&self, b: &Block) -> bool {
        b.stmts.iter().any(|s| self.k.gpu_stmts.contains(&s.id))
    }

    fn call_stmt(&mut self, s: &Stmt, e: &Expr) {
        let bt = self.bt.expr(e).max(self.ctl);
        let eff = self.effects.expr(e);
        // assert / error は GPU では無視する。
        if let Some(StaticValue::Builtin(b)) = self.resolver.callee(e) {
            if matches!(b.name, "assert" | "error") {
                if bt == Bt::P {
                    self.warn(
                        "gpu-assert-ignored",
                        s.span,
                        format!("`{}` depends on GPU-only data and is ignored on the GPU", b.name),
                    );
                } else if self.in_func.is_none() {
                    self.replay_leaves.insert(s.id);
                }
                return;
            }
        }
        if self.placement(e) == Placement::Gpu && self.is_gpu_call(e) {
            self.gexpr_multi(e);
            self.mark_gpu(s);
            return;
        }
        // CPU でしか実行できない呼び出し。
        if self.in_func.is_some() {
            self.error(
                "gpu-cpu-call",
                s.span,
                "this call needs the CPU and cannot run inside a function called on the GPU",
            );
            return;
        }
        if bt == Bt::P {
            if eff <= Effect::ReadOnly {
                // 結果を使わない純粋な呼び出しは、GPU 側では単に省く。
                return;
            }
            self.error(
                "cpu-call-gpu-data",
                s.span,
                "this call has side effects and depends on GPU-only data, so it can run neither on the CPU nor on the GPU",
            );
            return;
        }
        if self.in_p_loop && eff > Effect::ReadOnly {
            self.error(
                "cpu-call-in-gpu-loop",
                s.span,
                "this call has side effects inside a loop controlled by GPU-only data",
            );
            return;
        }
        self.replay_leaves.insert(s.id);
    }

    fn is_gpu_call(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Spec(site) => matches!(
                (self.inputs.targets[*site as usize], self.inputs.site_impls[*site as usize]),
                (SiteTarget::Gpu, _) | (SiteTarget::Cpu, SiteImpl::CpuFunc(_))
            ),
            ExprKind::Call { .. } | ExprKind::Method { .. } => matches!(
                self.resolver.callee(e),
                Some(StaticValue::Func(_)) | Some(StaticValue::Builtin(_))
            ),
            _ => false,
        }
    }

    fn assign(&mut self, s: &Stmt, targets: &[Target], exprs: &[Expr]) {
        let value_bt = exprs.iter().fold(Bt::U, |a, e| a.max(self.bt.expr(e))).max(self.ctl);
        let mut gpu = false;
        let mut cpu = false;
        // 値ごとの GPU 型(多値の展開を含む)。
        let mut gpu_value_types: Option<Vec<GpuTy>> = None;
        let all_targets_gpu = targets.iter().all(|t| match t {
            Target::Var(v) => !self.cpu_defined.contains_key(v),
            Target::LocalIndex(..) => true,
            _ => false,
        });
        if all_targets_gpu {
            let mut types = Vec::new();
            for (i, e) in exprs.iter().enumerate() {
                if i + 1 == exprs.len() && targets.len() > exprs.len() && e.is_multi(self.program) {
                    types.extend(self.gexpr_multi(e));
                } else {
                    types.push(self.gexpr(e, &GpuTy::Unknown));
                }
            }
            gpu_value_types = Some(types);
            gpu = true;
        }
        for (i, t) in targets.iter().enumerate() {
            match t {
                Target::Var(v) => {
                    if self.cpu_defined.contains_key(v) {
                        cpu = true;
                    } else if let Some(types) = &gpu_value_types {
                        let ty = types.get(i).cloned().unwrap_or(GpuTy::Cpu);
                        if !ty.is_storable_or_unknown() {
                            let name = self.program.var(*v).name.clone();
                            self.error(
                                "gpu-type",
                                s.span,
                                format!("`{name}` would hold a value that cannot be represented on the GPU ({})", ty.describe()),
                            );
                        }
                        let prev_ty = self.k.var_types.get(v).cloned();
                        let merged = match prev_ty {
                            Some(prev) => match prev.unify(&ty) {
                                Some(t) => t,
                                None => {
                                    let name = self.program.var(*v).name.clone();
                                    self.error(
                                        "gpu-type-mismatch",
                                        s.span,
                                        format!(
                                            "`{name}` is assigned values of different types ({} and {}) on the GPU",
                                            prev.describe(),
                                            ty.describe()
                                        ),
                                    );
                                    prev
                                }
                            },
                            None => ty,
                        };
                        self.k.var_types.insert(*v, merged);
                    }
                }
                Target::LocalIndex(obj, key) => {
                    let _ = self.gexpr(obj, &GpuTy::Unknown);
                    let _ = self.gexpr(key, &GpuTy::Unknown);
                }
                Target::Outer | Target::SharedIndex => {
                    cpu = true;
                    if value_bt == Bt::P {
                        // deps で弾かれているはずだが念のため。
                        self.error(
                            "gpu-value-to-cpu",
                            s.span,
                            "a value computed on the GPU cannot be stored where CPU code reads it",
                        );
                    }
                }
            }
        }
        if gpu && targets.iter().any(|t| matches!(t, Target::Var(_) | Target::LocalIndex(..))) {
            self.mark_gpu(s);
        }
        // CPU 側の状態(外側の変数・共有テーブル)を更新する、または副作用のある代入は必ず再生する。
        // CPU でしか値が分からない本体の local の定義だけなら、記録で必要になったときに
        // finish_replay が再生に加える。
        let updates_cpu_state = targets
            .iter()
            .any(|t| matches!(t, Target::Outer | Target::SharedIndex));
        let cpu = cpu && (updates_cpu_state || self.effects.stmt(s) > Effect::ReadOnly);
        if cpu || value_bt < Bt::P {
            if cpu {
                if self.in_p_loop || self.ctl == Bt::P {
                    self.error(
                        "cpu-state-in-gpu-control",
                        s.span,
                        "CPU-side state is updated under a condition that depends on GPU-only data",
                    );
                }
                self.replay_leaves.insert(s.id);
            }
        }
    }

    // ---- 式(GPU 側) ------------------------------------------------------------

    fn record_type(&mut self, e: &Expr, ty: GpuTy) -> GpuTy {
        self.expr_types.insert(ExprKey::of(e), ty.clone());
        ty
    }

    /// 多値の式の型(関数呼び出し / GPU 関数 / 多値を返す組み込み)。
    fn gexpr_multi(&mut self, e: &Expr) -> Vec<GpuTy> {
        match &e.kind {
            ExprKind::Spec(site) if self.inputs.targets[*site as usize] == SiteTarget::Gpu => {
                let first = self.gexpr(e, &GpuTy::Unknown);
                let rets: Vec<GpuTy> = self.inputs.gpu_returns[*site as usize]
                    .iter()
                    .map(GpuTy::from_spec)
                    .collect();
                if rets.is_empty() {
                    let _ = first;
                }
                rets
            }
            ExprKind::Call { .. } | ExprKind::Method { .. } | ExprKind::Spec(_) => {
                let first = self.gexpr(e, &GpuTy::Unknown);
                match self.k.cpu_refs.get(&ExprKey::of(e)) {
                    Some(_) => vec![first],
                    None => match self.callee_func(e) {
                        Some(f) => self
                            .k
                            .func_types
                            .get(&f)
                            .map(|s| s.returns.clone())
                            .unwrap_or_else(|| vec![first]),
                        None => match self.resolver.callee(e) {
                            Some(StaticValue::Builtin(b)) if matches!(b.name, "math.modf" | "math.frexp") => {
                                vec![GpuTy::Float, GpuTy::Float]
                            }
                            _ => vec![first],
                        },
                    },
                }
            }
            _ => vec![self.gexpr(e, &GpuTy::Unknown)],
        }
    }

    fn callee_func(&self, e: &Expr) -> Option<FuncId> {
        match &e.kind {
            ExprKind::Spec(site) => match self.inputs.site_impls[*site as usize] {
                SiteImpl::CpuFunc(f) if self.inputs.targets[*site as usize] == SiteTarget::Cpu => Some(f),
                _ => None,
            },
            _ => match self.resolver.callee(e) {
                Some(StaticValue::Func(f)) => Some(f),
                _ => None,
            },
        }
    }

    /// 式を GPU で求められるようにし、その(最初の値の)型を返す。
    fn gexpr(&mut self, e: &Expr, expect: &GpuTy) -> GpuTy {
        if let Some(r) = self.k.cpu_refs.get(&ExprKey::of(e)) {
            let ty = match r {
                CpuRef::Hoist(i) => self.k.hoists[*i].ty.clone(),
                CpuRef::Record(i) => self.k.records[*i].ty.clone(),
            };
            return ty;
        }
        if !self.can_gpu(e, 0) {
            if self.can_be_cpu_value(e) {
                return self.cpu_value(e, expect);
            }
            return self.gpu_failure(e);
        }
        let ty = self.gexpr_inner(e, expect);
        self.record_type(e, ty)
    }

    fn gpu_failure(&mut self, e: &Expr) -> GpuTy {
        let bt = self.bt.expr(e);
        let msg = if self.in_func.is_some() {
            "this expression needs the CPU and cannot be used inside a function called on the GPU"
                .to_string()
        } else if bt == Bt::P {
            "this expression needs the CPU but depends on data that only exists on the GPU".to_string()
        } else {
            format!(
                "this value ({}) cannot be passed to the GPU",
                self.guess_type(e).describe()
            )
        };
        self.error("gpu-unsupported", e.span, msg);
        GpuTy::Unknown
    }

    fn gexpr_inner(&mut self, e: &Expr, expect: &GpuTy) -> GpuTy {
        match &e.kind {
            ExprKind::Number(_) => GpuTy::Float,
            ExprKind::Bool(_) => GpuTy::Bool,
            ExprKind::Paren(inner) => self.gexpr(inner, expect),
            ExprKind::Local(v) => {
                if self.dim_vars.contains(v) {
                    return GpuTy::Float;
                }
                let r = VarRef::Local(*v);
                if self.body_locals.contains(v) || self.k.privatized.contains(&r) || self.in_func_local(*v) {
                    let t = self.k.var_types.get(v).cloned().unwrap_or(GpuTy::Unknown);
                    let t = t.unify(expect).unwrap_or(t);
                    if self.k.privatized.contains(&r) || self.in_func_local(*v) || self.body_locals.contains(v) {
                        self.k.var_types.insert(*v, t.clone());
                    }
                    return t;
                }
                self.capture(e, CaptureSource::Var(r), expect)
            }
            ExprKind::Global(g) => match self.resolver.value_of(e) {
                Some(StaticValue::Builtin(b)) => GpuTy::Builtin(b),
                Some(StaticValue::Func(f)) => GpuTy::Func(f),
                _ => self.capture(e, CaptureSource::Var(VarRef::Global(g.clone())), expect),
            },
            ExprKind::Index { obj, key } => {
                if let Some(name) = external_name(e) {
                    return self.capture(e, CaptureSource::External(name.to_string()), expect);
                }
                if let Some(StaticValue::Builtin(b)) = self.resolver.value_of(e) {
                    return match b.gpu {
                        GpuImpl::Const(_) => GpuTy::Float,
                        _ => GpuTy::Builtin(b),
                    };
                }
                let obj_expect = match &key.kind {
                    ExprKind::Str(k) => {
                        GpuTy::Record(vec![(String::from_utf8_lossy(k).into_owned(), expect.clone())])
                    }
                    _ => GpuTy::Array(Box::new(expect.clone()), ArrLen::Param),
                };
                let tobj = self.gexpr(obj, &obj_expect);
                match (&tobj, &key.kind) {
                    (GpuTy::Record(fields), ExprKind::Str(k)) => {
                        let k = String::from_utf8_lossy(k);
                        match fields.iter().find(|(n, _)| *n == k) {
                            Some((_, t)) => t.clone(),
                            None => {
                                self.error(
                                    "gpu-field",
                                    e.span,
                                    format!("field `{k}` is not known on the GPU"),
                                );
                                GpuTy::Unknown
                            }
                        }
                    }
                    (GpuTy::Array(elem, _), _) => {
                        self.gexpr(key, &GpuTy::Float);
                        (**elem).clone()
                    }
                    (GpuTy::Unknown, _) => GpuTy::Unknown,
                    _ => {
                        self.error(
                            "gpu-index",
                            e.span,
                            format!("cannot index a {} on the GPU", tobj.describe()),
                        );
                        GpuTy::Unknown
                    }
                }
            }
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::Eq | BinOp::Ne if is_type_compare(self, lhs, rhs) => GpuTy::Bool,
                o if o.is_arith() => {
                    self.expect_float(lhs);
                    self.expect_float(rhs);
                    GpuTy::Float
                }
                BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                    self.expect_float(lhs);
                    self.expect_float(rhs);
                    GpuTy::Bool
                }
                BinOp::Eq | BinOp::Ne => {
                    let l = self.gexpr(lhs, &GpuTy::Unknown);
                    let r = self.gexpr(rhs, &l);
                    if !l.is_scalar_or_unknown() || !r.is_scalar_or_unknown() {
                        self.error("gpu-compare", e.span, "only numbers and booleans can be compared on the GPU");
                    }
                    GpuTy::Bool
                }
                // `c and a or b`(三項演算子の慣用句): (c と a が真) ? a : b。
                BinOp::Or if matches!(&lhs.kind, ExprKind::Binary { op: BinOp::And, .. }) => {
                    let ExprKind::Binary { lhs: c, rhs: a, .. } = &lhs.kind else {
                        unreachable!()
                    };
                    self.gexpr(c, &GpuTy::Unknown);
                    self.short_circuit_depth += 1;
                    let at = self.gexpr(a, expect);
                    let bt = self.gexpr(rhs, &at);
                    self.short_circuit_depth -= 1;
                    match at.unify(&bt) {
                        Some(t) if t.is_scalar_or_unknown() => t,
                        _ => {
                            self.error(
                                "gpu-logic",
                                e.span,
                                format!(
                                    "`c and a or b` on the GPU needs a and b to have the same type (got {} and {})",
                                    at.describe(),
                                    bt.describe()
                                ),
                            );
                            GpuTy::Unknown
                        }
                    }
                }
                BinOp::And | BinOp::Or => {
                    let l = self.gexpr(lhs, expect);
                    // 右辺は条件付きで評価される(短絡評価)。
                    self.short_circuit_depth += 1;
                    let r = self.gexpr(rhs, &l);
                    self.short_circuit_depth -= 1;
                    match l.unify(&r) {
                        Some(t) if t.is_scalar_or_unknown() => t,
                        _ => {
                            self.error(
                                "gpu-logic",
                                e.span,
                                format!(
                                    "`{}` on the GPU needs both operands to have the same type (got {} and {})",
                                    op.lua_symbol(),
                                    l.describe(),
                                    r.describe()
                                ),
                            );
                            GpuTy::Unknown
                        }
                    }
                }
                _ => GpuTy::Unknown,
            },
            ExprKind::Unary { op, expr } => match op {
                UnOp::Neg => {
                    self.expect_float(expr);
                    GpuTy::Float
                }
                UnOp::Not => {
                    self.gexpr(expr, &GpuTy::Unknown);
                    GpuTy::Bool
                }
                UnOp::Len => {
                    self.gexpr(expr, &GpuTy::Array(Box::new(GpuTy::Unknown), ArrLen::Param));
                    GpuTy::Float
                }
            },
            ExprKind::Table(fields) => {
                if fields.iter().all(|f| matches!(f, TableField::Positional(_))) {
                    let mut elem = GpuTy::Unknown;
                    for f in fields {
                        if let TableField::Positional(v) = f {
                            let t = self.gexpr(v, &elem);
                            elem = elem.unify(&t).unwrap_or_else(|| {
                                self.error("gpu-table", v.span, "array elements must all have the same type on the GPU");
                                GpuTy::Unknown
                            });
                        }
                    }
                    let cap = (fields.len() as u32).max(1);
                    GpuTy::Array(Box::new(elem), ArrLen::Local(cap))
                } else {
                    let mut out = Vec::new();
                    for f in fields {
                        if let TableField::Named(k, v) = f {
                            let t = self.gexpr(v, &GpuTy::Unknown);
                            out.push((k.clone(), t));
                        }
                    }
                    GpuTy::Record(out)
                }
            }
            ExprKind::Call { args, .. } | ExprKind::Method { args, .. } => {
                match self.resolver.callee(e) {
                    Some(StaticValue::Builtin(b)) => self.builtin_call(e, b, args),
                    Some(StaticValue::Func(f)) => self.user_call(e, f, args),
                    _ => GpuTy::Unknown,
                }
            }
            ExprKind::Spec(site) => {
                let i = *site as usize;
                let args = self.program.site(*site).args.clone();
                match self.inputs.targets[i] {
                    SiteTarget::Gpu => {
                        let params = self.inputs.gpu_params[i].clone();
                        for (a, p) in args.iter().zip(params.iter()) {
                            let pt = GpuTy::from_spec(p);
                            let t = self.gexpr(a, &pt);
                            if t.unify(&pt).is_none() {
                                self.error(
                                    "gpu-arg-type",
                                    a.span,
                                    format!("expected {} but got {}", pt.describe(), t.describe()),
                                );
                            }
                        }
                        self.inputs.gpu_returns[i]
                            .first()
                            .map(GpuTy::from_spec)
                            .unwrap_or(GpuTy::Cpu)
                    }
                    _ => match self.inputs.site_impls[i] {
                        SiteImpl::CpuFunc(f) => self.user_call(e, f, &args),
                        _ => GpuTy::Unknown,
                    },
                }
            }
            _ => GpuTy::Unknown,
        }
    }

    fn expect_float(&mut self, e: &Expr) {
        let t = self.gexpr(e, &GpuTy::Float);
        if t.unify(&GpuTy::Float).is_none() {
            self.error(
                "gpu-arith",
                e.span,
                format!("arithmetic on a {} value", t.describe()),
            );
        }
    }

    fn builtin_call(&mut self, e: &Expr, b: &'static crate::stdlib::Builtin, args: &[Expr]) -> GpuTy {
        match b.name {
            "assert" => {
                let t = args.first().map(|a| self.gexpr(a, &GpuTy::Unknown)).unwrap_or(GpuTy::Unknown);
                self.warn("gpu-assert-ignored", e.span, "`assert` is not checked on the GPU");
                t
            }
            "tonumber" => args.first().map(|a| self.gexpr(a, &GpuTy::Float)).unwrap_or(GpuTy::Float),
            "rawequal" => {
                for a in args {
                    self.gexpr(a, &GpuTy::Unknown);
                }
                GpuTy::Bool
            }
            "rawget" => {
                let t = args.first().map(|a| self.gexpr(a, &GpuTy::Array(Box::new(GpuTy::Unknown), ArrLen::Param)));
                if let Some(k) = args.get(1) {
                    self.gexpr(k, &GpuTy::Float);
                }
                match t {
                    Some(GpuTy::Array(elem, _)) => *elem,
                    _ => GpuTy::Unknown,
                }
            }
            "string.len" | "string.byte" => {
                for a in args.iter().skip(1) {
                    self.gexpr(a, &GpuTy::Float);
                }
                GpuTy::Float
            }
            "error" => {
                self.warn("gpu-assert-ignored", e.span, "`error` is ignored on the GPU");
                GpuTy::Unknown
            }
            "select" => {
                self.error("gpu-unsupported", e.span, "`select` cannot be used on the GPU");
                GpuTy::Unknown
            }
            _ => {
                for a in args {
                    self.expect_float(a);
                }
                GpuTy::Float
            }
        }
    }

    fn in_func_local(&self, v: VarId) -> bool {
        match self.in_func {
            Some(f) => self.program.var(v).func == f,
            None => false,
        }
    }

    /// ユーザー関数の呼び出し。関数を GPU 用に解析し、シグネチャを決める。
    fn user_call(&mut self, e: &Expr, f: FuncId, args: &[Expr]) -> GpuTy {
        let mut arg_types = Vec::new();
        for (i, a) in args.iter().enumerate() {
            if i + 1 == args.len() && a.is_multi(self.program) {
                arg_types.extend(self.gexpr_multi(a));
            } else {
                arg_types.push(self.gexpr(a, &GpuTy::Unknown));
            }
        }
        match self.func_sig(f, &arg_types, e.span) {
            Some(sig) => sig.returns.first().cloned().unwrap_or(GpuTy::Cpu),
            None => GpuTy::Unknown,
        }
    }

    fn func_sig(&mut self, f: FuncId, args: &[GpuTy], span: Span) -> Option<FuncSig> {
        let func = self.program.func(f).clone();
        let mut params: Vec<GpuTy> = func
            .params
            .iter()
            .enumerate()
            .map(|(i, _)| args.get(i).cloned().unwrap_or(GpuTy::Cpu))
            .collect();
        if let Some(prev) = self.k.func_types.get(&f).cloned() {
            // 1 関数につき 1 つの特殊化。型が合わなければエラー。
            let mut ok = true;
            for (p, a) in prev.params.iter().zip(params.iter_mut()) {
                match p.unify(a) {
                    Some(t) => *a = t,
                    None => ok = false,
                }
            }
            if !ok {
                let name = func.name.clone().unwrap_or_else(|| "function".into());
                self.error(
                    "gpu-specialization",
                    span,
                    format!("`{name}` is called on the GPU with different argument types"),
                );
                return None;
            }
            if self.funcs_in_progress.contains(&f) || prev.params == params {
                return Some(prev);
            }
        }
        if self.funcs_in_progress.contains(&f) {
            let name = func.name.clone().unwrap_or_else(|| "function".into());
            self.error("gpu-recursion", span, format!("recursive function `{name}` cannot run on the GPU"));
            return None;
        }
        if func.is_vararg {
            self.error("gpu-vararg", span, "functions with `...` cannot run on the GPU");
            return None;
        }
        for (v, t) in func.params.iter().zip(params.iter()) {
            if matches!(t, GpuTy::Cpu) {
                let name = self.program.var(*v).name.clone();
                self.error("gpu-arg-type", span, format!("argument `{name}` cannot be passed on the GPU"));
                return None;
            }
            self.k.var_types.insert(*v, t.clone());
        }
        self.funcs_in_progress.insert(f);
        let saved_func = self.in_func.replace(f);
        let saved_ctl = self.ctl;
        let saved_loop = self.record_loop;
        self.record_loop = None;
        // 関数内の local はすべて GPU 側(CPU の値は使えない)。
        let sig_placeholder = FuncSig {
            params: params.clone(),
            returns: vec![GpuTy::Unknown],
        };
        self.k.func_types.insert(f, sig_placeholder);
        self.block(&func.body);
        let returns = self.return_types(&func.body);
        self.in_func = saved_func;
        self.ctl = saved_ctl;
        self.record_loop = saved_loop;
        self.funcs_in_progress.remove(&f);
        let sig = FuncSig { params, returns };
        self.k.func_types.insert(f, sig.clone());
        if !self.k.funcs.contains(&f) {
            self.k.funcs.push(f);
        }
        Some(sig)
    }

    fn return_types(&mut self, b: &Block) -> Vec<GpuTy> {
        let mut out: Option<Vec<GpuTy>> = None;
        let mut rets = Vec::new();
        collect_returns(b, &mut rets);
        for r in rets {
            let types: Vec<GpuTy> = r
                .iter()
                .map(|e| self.expr_types.get(&ExprKey::of(e)).cloned().unwrap_or(GpuTy::Unknown))
                .collect();
            out = Some(match out {
                None => types,
                Some(prev) => {
                    if prev.len() != types.len() {
                        self.error("gpu-return-arity", r.first().map(|e| e.span).unwrap_or_default(), "all `return` statements of a GPU function must return the same number of values");
                        prev
                    } else {
                        prev.iter()
                            .zip(types.iter())
                            .map(|(a, b)| a.unify(b).unwrap_or(GpuTy::Unknown))
                            .collect()
                    }
                }
            });
        }
        out.unwrap_or_default()
    }

    /// 外側の値を捕捉する。型は使われ方(expect)と宣言から決める。
    fn capture(&mut self, e: &Expr, source: CaptureSource, expect: &GpuTy) -> GpuTy {
        if self.in_func.is_some() {
            // 関数の upvalue も、カーネルの外の値として捕捉する。
        }
        let key = match &source {
            CaptureSource::Var(VarRef::Local(v)) => format!("local:{v}"),
            CaptureSource::Var(VarRef::Global(g)) => format!("global:{g}"),
            CaptureSource::External(n) => format!("ext:{n}"),
        };
        let declared = match &source {
            CaptureSource::External(n) => self
                .inputs
                .externals
                .get(n)
                .map(GpuTy::from_spec)
                .unwrap_or(GpuTy::Unknown),
            _ => self.capture_guess(e),
        }
        .as_param();
        let prev = self
            .k
            .captures
            .get(&key)
            .map(|c| c.ty.clone())
            .unwrap_or(declared);
        let ty = match prev.unify(&expect.as_param()) {
            Some(t) => t,
            None => {
                self.error(
                    "gpu-capture-type",
                    e.span,
                    format!(
                        "value is used as {} on the GPU but it is a {}",
                        expect.describe(),
                        prev.describe()
                    ),
                );
                prev
            }
        };
        if matches!(ty, GpuTy::Cpu) {
            self.error(
                "gpu-capture-type",
                e.span,
                "this value cannot be passed to the GPU (only numbers, booleans and tables of them)",
            );
        }
        // 配列は params の可変長領域で渡す。
        let ty = match ty {
            GpuTy::Array(elem, ArrLen::Local(_)) => GpuTy::Array(elem, ArrLen::Param),
            t => t,
        };
        let name = format!("lg_cap{}", self.k.captures.len());
        let entry = self.k.captures.entry(key).or_insert(Capture {
            source,
            ty: ty.clone(),
            name,
        });
        entry.ty = ty.clone();
        ty
    }

    /// CPU で計算する値(持ち出し or 記録)にする。
    fn cpu_value(&mut self, e: &Expr, expect: &GpuTy) -> GpuTy {
        // 再代入されない本体の local で、初期値がループ不変かつ副作用の無いものは、初期値ごと持ち出す
        // (例: `local gain = tonumber(io.open(path):read("*a"))` → ディスパッチ前に 1 回だけ読む)。
        if let ExprKind::Local(v) = e.kind {
            if self.body_locals.contains(&v)
                && self.cpu_defined.contains_key(&v)
                && !self.program.var(v).reassigned
            {
                if let Some(init) = self.local_init(v) {
                    let eff = self.effects.expr(&init);
                    let ok = eff == Effect::Pure || (eff == Effect::ReadOnly && !self.body_has_write);
                    if ok && self.is_invariant(&init) {
                        let ikey = ExprKey::of(&init);
                        let idx = match self.k.cpu_refs.get(&ikey) {
                            Some(CpuRef::Hoist(i)) => *i,
                            _ => {
                                let guess = self.guess_type(&init);
                                let ty = match guess.unify(expect) {
                                    Some(t) if t.is_scalar() => t,
                                    _ if guess.is_scalar() => guess,
                                    _ => GpuTy::Float,
                                };
                                let i = self.k.hoists.len();
                                self.k.hoists.push(CpuValue {
                                    expr: init.clone(),
                                    ty,
                                    record_loop: None,
                                    offset: 0,
                                });
                                self.k.cpu_refs.insert(ikey, CpuRef::Hoist(i));
                                i
                            }
                        };
                        let ty = self.k.hoists[idx].ty.clone();
                        self.k.cpu_refs.insert(ExprKey::of(e), CpuRef::Hoist(idx));
                        self.expr_types.insert(ExprKey::of(e), ty.clone());
                        return ty;
                    }
                }
            }
        }
        let guess = self.guess_type(e);
        let ty = match guess.unify(expect) {
            Some(t) if t.is_scalar() => t,
            _ if guess.is_scalar() => guess,
            _ if expect.is_scalar() => expect.clone(),
            _ => GpuTy::Float,
        };
        let eff = self.effects.expr(e);
        let invariant = self.is_invariant(e);
        let key = ExprKey::of(e);
        let hoist_ok = invariant
            && (eff == Effect::Pure || (eff == Effect::ReadOnly && !self.body_has_write));
        if hoist_ok {
            let idx = self.k.hoists.len();
            self.k.hoists.push(CpuValue {
                expr: e.clone(),
                ty: ty.clone(),
                record_loop: None,
                offset: 0,
            });
            self.k.cpu_refs.insert(key, CpuRef::Hoist(idx));
            self.expr_types.insert(key, ty.clone());
            return ty;
        }
        // 記録(CPU の再生で反復ごとに計算する)。
        if self.short_circuit_depth > 0 && eff > Effect::ReadOnly {
            self.error(
                "cpu-value-short-circuit",
                e.span,
                "a CPU-only value with side effects on the right of `and` / `or` cannot be used on the GPU",
            );
            return ty;
        }
        if self.ctl == Bt::P && eff > Effect::ReadOnly {
            self.error(
                "cpu-value-in-gpu-control",
                e.span,
                "this CPU-only value is evaluated under a condition that depends on GPU-only data",
            );
            return ty;
        }
        if self.in_p_loop {
            self.error(
                "cpu-value-in-gpu-loop",
                e.span,
                "this CPU-only value is evaluated inside a loop whose iteration count depends on GPU-only data",
            );
            return ty;
        }
        if let Some(why) = self.record_blocked {
            self.error(
                "cpu-value-unsupported-loop",
                e.span,
                format!("values that need the CPU (other than loop-invariant ones) cannot be used inside {why} on the GPU yet"),
            );
            return ty;
        }
        if let Some(sid) = self.cur_stmt {
            self.record_stmts.insert(sid);
        }
        let (record_loop, offset) = match self.record_loop {
            Some(l) => {
                let lp = &mut self.k.record_loops[l];
                let off = lp.stride;
                lp.stride += 1;
                (Some(l), off)
            }
            None => {
                let off = self.next_offset;
                self.next_offset += 1;
                (None, off)
            }
        };
        let idx = self.k.records.len();
        self.k.records.push(CpuValue {
            expr: e.clone(),
            ty: ty.clone(),
            record_loop,
            offset,
        });
        self.k.cpu_refs.insert(key, CpuRef::Record(idx));
        self.expr_types.insert(key, ty.clone());
        // 記録の式が使う本体の local は、再生でも定義する必要がある。
        let mut vars = Vec::new();
        free_locals(self.program, e, &mut vars);
        for v in vars {
            if self.body_locals.contains(&v) {
                self.replay_needed_vars.insert(v);
            }
        }
        ty
    }

    /// 本体に入る前に評価してよい(ループ不変な)式か。
    fn is_invariant(&self, e: &Expr) -> bool {
        if self.bt.expr(e) != Bt::U {
            return false;
        }
        let mut vars = Vec::new();
        free_locals(self.program, e, &mut vars);
        let locals_ok = vars.iter().all(|v| {
            !self.body_locals.contains(v)
                && !self.dim_vars.contains(v)
                && !self.k.carried.contains(&VarRef::Local(*v))
                && !self.k.privatized.contains(&VarRef::Local(*v))
        });
        let mut globals = Vec::new();
        free_globals(self.program, e, &mut globals);
        let globals_ok = globals
            .iter()
            .all(|g| !self.k.carried.contains(&VarRef::Global(g.clone())));
        locals_ok && globals_ok
    }

    // ---- 再生の範囲 ---------------------------------------------------------------

    /// 再生が必要な葉の文から、必要な定義(本体の local)と制御構造をたどって再生の範囲を決める。
    fn finish_replay(&mut self, body: &Block) {
        // 記録が使う local を定義する文を、再生の葉に加える(不動点)。
        let mut needed = self.replay_needed_vars.clone();
        loop {
            let mut changed = false;
            let mut defs: Vec<(StmtId, Vec<VarId>)> = Vec::new();
            collect_defs(body, &mut defs);
            for (sid, vars) in &defs {
                if vars.iter().any(|v| needed.contains(v)) && self.replay_leaves.insert(*sid) {
                    changed = true;
                }
            }
            // 再生する文(と再生する飛び越しの条件)が読む本体の local も必要。
            let combined: HashSet<StmtId> =
                self.replay_leaves.union(&self.replay_controls).copied().collect();
            let mut reads = Vec::new();
            collect_stmt_reads(self.program, body, &combined, &mut reads);
            for v in reads {
                if self.body_locals.contains(&v) && needed.insert(v) {
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        // 再生する文が P の値を読んでいたらエラー(CPU では計算できない)。
        let mut bad = Vec::new();
        let combined: HashSet<StmtId> =
            self.replay_leaves.union(&self.replay_controls).copied().collect();
        check_replay_reads(self.program, body, &combined, &self.k.var_bt, &self.dim_vars, &mut bad);
        for (span, name) in bad {
            self.error(
                "cpu-needs-gpu-value",
                span,
                format!("`{name}` is computed on the GPU but this CPU-side statement needs it"),
            );
        }
        let has_state = !self.k.carried.is_empty() || !self.k.written_tables.is_empty();
        self.k.needs_replay = !self.replay_leaves.is_empty() || !self.k.records.is_empty() || has_state;
        self.k.replay_leaves = std::mem::take(&mut self.replay_leaves);
        self.k.record_stmts = std::mem::take(&mut self.record_stmts);
        self.k.replay_controls = std::mem::take(&mut self.replay_controls);
    }
}

#[derive(Clone, Debug)]
enum Target {
    Var(VarId),
    Outer,
    LocalIndex(Expr, Expr),
    SharedIndex,
}

trait TyExt {
    fn is_storable_or_unknown(&self) -> bool;
    fn is_scalar_or_unknown(&self) -> bool;
}

impl TyExt for GpuTy {
    fn is_storable_or_unknown(&self) -> bool {
        match self {
            GpuTy::Unknown => true,
            GpuTy::Array(e, _) => e.is_storable_or_unknown(),
            GpuTy::Record(f) => f.iter().all(|(_, t)| t.is_storable_or_unknown()),
            t => t.is_storable(),
        }
    }
    fn is_scalar_or_unknown(&self) -> bool {
        matches!(self, GpuTy::Unknown | GpuTy::Float | GpuTy::Bool)
    }
}

/// `__lg_ext.name`(外部値の参照)なら name。
pub fn external_name(e: &Expr) -> Option<&str> {
    match &e.kind {
        ExprKind::Index { obj, key } => match (&obj.kind, &key.kind) {
            (ExprKind::Global(g), ExprKind::Str(k)) if g == "__lg_ext" => std::str::from_utf8(k).ok(),
            _ => None,
        },
        _ => None,
    }
}

fn capture_key_of(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Local(v) => format!("local:{v}"),
        ExprKind::Global(g) => format!("global:{g}"),
        _ => String::new(),
    }
}

/// `type(x) == "number"` のような、静的に畳み込める型比較か。
fn is_type_compare(c: &Classifier, lhs: &Expr, rhs: &Expr) -> bool {
    let is_type_call = |e: &Expr| {
        matches!(c.resolver.callee(e), Some(StaticValue::Builtin(b)) if b.name == "type")
    };
    (is_type_call(lhs) && matches!(rhs.kind, ExprKind::Str(_)))
        || (is_type_call(rhs) && matches!(lhs.kind, ExprKind::Str(_)))
}

fn call_arg(e: &Expr, i: usize) -> Option<&Expr> {
    match &e.kind {
        ExprKind::Call { args, .. } | ExprKind::Method { args, .. } => args.get(i),
        _ => None,
    }
}

fn find_local_init(b: &Block, v: VarId) -> Option<Expr> {
    for s in &b.stmts {
        match &s.kind {
            StmtKind::Local { vars, exprs } => {
                if let Some(i) = vars.iter().position(|x| *x == v) {
                    return exprs.get(i).cloned();
                }
            }
            StmtKind::Do(b) => {
                if let Some(e) = find_local_init(b, v) {
                    return Some(e);
                }
            }
            StmtKind::If { arms, else_block } => {
                for (_, blk) in arms {
                    if let Some(e) = find_local_init(blk, v) {
                        return Some(e);
                    }
                }
                if let Some(blk) = else_block {
                    if let Some(e) = find_local_init(blk, v) {
                        return Some(e);
                    }
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::Repeat { body, .. }
            | StmtKind::NumFor { body, .. }
            | StmtKind::GenFor { body, .. } => {
                if let Some(e) = find_local_init(body, v) {
                    return Some(e);
                }
            }
            _ => {}
        }
    }
    None
}

/// 式が読む local(関数式の中は含まない)。
pub fn free_locals(program: &Program, e: &Expr, out: &mut Vec<VarId>) {
    if let ExprKind::Local(v) = e.kind {
        out.push(v);
    }
    walk_expr_children(program, e, &mut |c| free_locals(program, c, out));
}

pub fn free_globals(program: &Program, e: &Expr, out: &mut Vec<String>) {
    if let ExprKind::Global(g) = &e.kind {
        out.push(g.clone());
    }
    walk_expr_children(program, e, &mut |c| free_globals(program, c, out));
}

fn collect_returns<'b>(b: &'b Block, out: &mut Vec<&'b [Expr]>) {
    for s in &b.stmts {
        match &s.kind {
            StmtKind::Return(es) => out.push(es),
            StmtKind::Do(b) => collect_returns(b, out),
            StmtKind::If { arms, else_block } => {
                for (_, blk) in arms {
                    collect_returns(blk, out);
                }
                if let Some(blk) = else_block {
                    collect_returns(blk, out);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::Repeat { body, .. }
            | StmtKind::NumFor { body, .. }
            | StmtKind::GenFor { body, .. } => collect_returns(body, out),
            _ => {}
        }
    }
}

/// 各葉の文が定義する本体の local。
fn collect_defs(b: &Block, out: &mut Vec<(StmtId, Vec<VarId>)>) {
    for s in &b.stmts {
        match &s.kind {
            StmtKind::Local { vars, .. } => out.push((s.id, vars.clone())),
            StmtKind::Assign { targets, .. } => out.push((
                s.id,
                targets
                    .iter()
                    .filter_map(|t| match t {
                        LValue::Local(v, _) => Some(*v),
                        _ => None,
                    })
                    .collect(),
            )),
            StmtKind::LocalFunction { var, .. } => out.push((s.id, vec![*var])),
            StmtKind::Do(b) => collect_defs(b, out),
            StmtKind::If { arms, else_block } => {
                for (_, blk) in arms {
                    collect_defs(blk, out);
                }
                if let Some(blk) = else_block {
                    collect_defs(blk, out);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::Repeat { body, .. }
            | StmtKind::NumFor { body, .. }
            | StmtKind::GenFor { body, .. } => collect_defs(body, out),
            _ => {}
        }
    }
}

/// 再生する葉の文が読む local(制御構造の条件も含む)。
fn collect_stmt_reads(program: &Program, b: &Block, leaves: &HashSet<StmtId>, out: &mut Vec<VarId>) {
    for s in &b.stmts {
        let contains = stmt_contains_leaf(s, leaves);
        match &s.kind {
            StmtKind::Local { exprs, .. } | StmtKind::Return(exprs) if leaves.contains(&s.id) => {
                exprs.iter().for_each(|e| free_locals(program, e, out))
            }
            StmtKind::Assign { targets, exprs } if leaves.contains(&s.id) => {
                exprs.iter().for_each(|e| free_locals(program, e, out));
                for t in targets {
                    if let LValue::Index { obj, key, .. } = t {
                        free_locals(program, obj, out);
                        free_locals(program, key, out);
                    }
                }
            }
            StmtKind::Call(e) if leaves.contains(&s.id) => free_locals(program, e, out),
            StmtKind::GenFor { exprs, body, .. } if leaves.contains(&s.id) => {
                exprs.iter().for_each(|e| free_locals(program, e, out));
                let all: HashSet<StmtId> = all_stmt_ids(body);
                collect_stmt_reads(program, body, &all, out);
            }
            StmtKind::Do(b) => collect_stmt_reads(program, b, leaves, out),
            StmtKind::If { arms, else_block } if contains => {
                for (c, blk) in arms {
                    free_locals(program, c, out);
                    collect_stmt_reads(program, blk, leaves, out);
                }
                if let Some(blk) = else_block {
                    collect_stmt_reads(program, blk, leaves, out);
                }
            }
            StmtKind::While { cond, body } | StmtKind::Repeat { body, cond } if contains => {
                free_locals(program, cond, out);
                collect_stmt_reads(program, body, leaves, out);
            }
            StmtKind::NumFor {
                start,
                limit,
                step,
                body,
                ..
            } if contains => {
                free_locals(program, start, out);
                free_locals(program, limit, out);
                if let Some(st) = step {
                    free_locals(program, st, out);
                }
                collect_stmt_reads(program, body, leaves, out);
            }
            _ => {}
        }
    }
}

fn all_stmt_ids(b: &Block) -> HashSet<StmtId> {
    let mut out = HashSet::new();
    fn go(b: &Block, out: &mut HashSet<StmtId>) {
        for s in &b.stmts {
            out.insert(s.id);
            match &s.kind {
                StmtKind::Do(b) => go(b, out),
                StmtKind::If { arms, else_block } => {
                    for (_, blk) in arms {
                        go(blk, out);
                    }
                    if let Some(blk) = else_block {
                        go(blk, out);
                    }
                }
                StmtKind::While { body, .. }
                | StmtKind::Repeat { body, .. }
                | StmtKind::NumFor { body, .. }
                | StmtKind::GenFor { body, .. } => go(body, out),
                _ => {}
            }
        }
    }
    go(b, &mut out);
    out
}

/// 文(またはその子孫)が再生する葉を含むか。
pub fn stmt_contains_leaf(s: &Stmt, leaves: &HashSet<StmtId>) -> bool {
    if leaves.contains(&s.id) {
        return true;
    }
    let any = |b: &Block| b.stmts.iter().any(|x| stmt_contains_leaf(x, leaves));
    match &s.kind {
        StmtKind::Do(b) => any(b),
        StmtKind::If { arms, else_block } => {
            arms.iter().any(|(_, b)| any(b)) || else_block.as_ref().is_some_and(any)
        }
        StmtKind::While { body, .. }
        | StmtKind::Repeat { body, .. }
        | StmtKind::NumFor { body, .. }
        | StmtKind::GenFor { body, .. } => any(body),
        _ => false,
    }
}

/// 再生する文(とその制御構造の条件)が P の local を読んでいないか。
fn check_replay_reads(
    program: &Program,
    b: &Block,
    leaves: &HashSet<StmtId>,
    var_bt: &HashMap<VarRef, Bt>,
    dim_vars: &HashSet<VarId>,
    bad: &mut Vec<(Span, String)>,
) {
    let check = |e: &Expr, bad: &mut Vec<(Span, String)>| {
        let mut vars = Vec::new();
        free_locals(program, e, &mut vars);
        for v in vars {
            if dim_vars.contains(&v) {
                continue;
            }
            if var_bt.get(&VarRef::Local(v)) == Some(&Bt::P) {
                bad.push((e.span, program.var(v).name.clone()));
            }
        }
    };
    for s in &b.stmts {
        if !stmt_contains_leaf(s, leaves) {
            continue;
        }
        match &s.kind {
            StmtKind::Local { exprs, .. } | StmtKind::Return(exprs) => {
                exprs.iter().for_each(|e| check(e, bad))
            }
            StmtKind::Assign { exprs, .. } => exprs.iter().for_each(|e| check(e, bad)),
            StmtKind::Call(e) => check(e, bad),
            StmtKind::Do(blk) => check_replay_reads(program, blk, leaves, var_bt, dim_vars, bad),
            StmtKind::If { arms, else_block } => {
                for (c, blk) in arms {
                    check(c, bad);
                    check_replay_reads(program, blk, leaves, var_bt, dim_vars, bad);
                }
                if let Some(blk) = else_block {
                    check_replay_reads(program, blk, leaves, var_bt, dim_vars, bad);
                }
            }
            StmtKind::While { cond, body } | StmtKind::Repeat { body, cond } => {
                check(cond, bad);
                check_replay_reads(program, body, leaves, var_bt, dim_vars, bad);
            }
            StmtKind::NumFor {
                start,
                limit,
                step,
                body,
                ..
            } => {
                check(start, bad);
                check(limit, bad);
                if let Some(st) = step {
                    check(st, bad);
                }
                check_replay_reads(program, body, leaves, var_bt, dim_vars, bad);
            }
            StmtKind::GenFor { exprs, body, .. } => {
                exprs.iter().for_each(|e| check(e, bad));
                check_replay_reads(program, body, leaves, var_bt, dim_vars, bad);
            }
            _ => {}
        }
    }
}

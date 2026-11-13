//! HIR → Lua のプリンター(ディスパッチ用ブロック・再生コード用)。
//!
//! 出力は改行を含まない 1 行(元のスクリプトの行番号を保つため、置き換えた範囲の改行数は
//! 呼び出し側で補う)。式は完全に括弧で囲み、優先順位の事故を防ぐ。

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::expand::{SiteExpansion, SiteTarget};
use crate::hir::*;
use crate::kernel::ExprKey;
use crate::pipeline::Analysis;
use crate::spec::is_lua_identifier;

pub struct LuaPrinter<'a> {
    pub a: &'a Analysis,
    /// 記録する式 → 記録先のスロットを表す Lua の式(`__lg_r(__lg_rec, <slot>, <expr>)` で包む)。
    pub records: HashMap<ExprKey, String>,
    /// 持ち出した式 → それを保持する Lua の変数名。
    pub hoists: HashMap<ExprKey, String>,
}

/// Lua の数値リテラル。
pub fn lua_number(v: f64) -> String {
    if v.is_nan() {
        "(0/0)".into()
    } else if v.is_infinite() {
        if v > 0.0 {
            "(1/0)".into()
        } else {
            "(-1/0)".into()
        }
    } else if v.fract() == 0.0 && v.abs() < 1e15 {
        if v == 0.0 && v.is_sign_negative() {
            "(-0)".into()
        } else {
            format!("{}", v as i64)
        }
    } else {
        format!("{v:?}")
    }
}

/// バイト列を Lua の文字列リテラルにする(改行・非 ASCII はエスケープ)。
pub fn lua_string(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() + 2);
    s.push('"');
    for &b in bytes {
        match b {
            b'\\' => s.push_str("\\\\"),
            b'"' => s.push_str("\\\""),
            b'\n' => s.push_str("\\n"),
            b'\r' => s.push_str("\\r"),
            b'\t' => s.push_str("\\t"),
            0x20..=0x7e => s.push(b as char),
            _ => {
                let _ = write!(s, "\\{b:03}");
            }
        }
    }
    s.push('"');
    s
}

impl<'a> LuaPrinter<'a> {
    pub fn new(a: &'a Analysis) -> Self {
        Self {
            a,
            records: HashMap::new(),
            hoists: HashMap::new(),
        }
    }

    fn program(&self) -> &Program {
        &self.a.program
    }

    pub fn var(&self, v: VarId) -> String {
        self.program().var(v).name.clone()
    }

    /// 式(記録・持ち出しの置き換えを含む)。
    pub fn expr(&self, e: &Expr) -> String {
        let key = ExprKey::of(e);
        if let Some(h) = self.hoists.get(&key) {
            return h.clone();
        }
        let inner = self.expr_raw(e);
        match self.records.get(&key) {
            Some(slot) => format!("__lg_r(__lg_rec, {slot}, {inner})"),
            None => inner,
        }
    }

    /// prefix expression として使える形(名前・括弧・チェーン)。
    fn prefix(&self, e: &Expr) -> String {
        let s = self.expr(e);
        let key = ExprKey::of(e);
        let plain = !self.records.contains_key(&key) && !self.hoists.contains_key(&key);
        match &e.kind {
            ExprKind::Local(_)
            | ExprKind::Global(_)
            | ExprKind::Index { .. }
            | ExprKind::Call { .. }
            | ExprKind::Method { .. }
            | ExprKind::Paren(_)
                if plain =>
            {
                s
            }
            ExprKind::Spec(_) if plain => s,
            _ => format!("({s})"),
        }
    }

    fn args(&self, args: &[Expr]) -> String {
        args.iter().map(|a| self.expr(a)).collect::<Vec<_>>().join(", ")
    }

    pub fn expr_raw(&self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Nil => "nil".into(),
            ExprKind::Bool(b) => b.to_string(),
            ExprKind::Number(n) => lua_number(*n),
            ExprKind::CData(t) => t.clone(),
            ExprKind::Str(s) => lua_string(s),
            ExprKind::Vararg => "...".into(),
            ExprKind::Local(v) => self.var(*v),
            ExprKind::Global(g) => g.clone(),
            ExprKind::Index { obj, key } => match &key.kind {
                ExprKind::Str(k)
                    if std::str::from_utf8(k).is_ok_and(is_lua_identifier)
                        && !self.records.contains_key(&ExprKey::of(key)) =>
                {
                    format!("{}.{}", self.prefix(obj), String::from_utf8_lossy(k))
                }
                _ => format!("{}[{}]", self.prefix(obj), self.expr(key)),
            },
            ExprKind::Call { func, args } => {
                // 同梱した dofile。
                if let (ExprKind::Global(g), [Expr { kind: ExprKind::Str(p), .. }]) =
                    (&func.kind, args.as_slice())
                {
                    if g == "dofile" {
                        let p = String::from_utf8_lossy(p);
                        if let Some(idx) = self.program().dofiles.get_index_of(p.as_ref()) {
                            return format!("__lg_df[{}]()", idx + 1);
                        }
                    }
                }
                format!("{}({})", self.prefix(func), self.args(args))
            }
            ExprKind::Method { obj, name, args } => {
                format!("{}:{}({})", self.prefix(obj), name, self.args(args))
            }
            ExprKind::Spec(site) => self.spec(*site),
            ExprKind::Function(f) => self.function(*f, None),
            ExprKind::Binary { op, lhs, rhs } => {
                format!("({} {} {})", self.expr(lhs), op.lua_symbol(), self.expr(rhs))
            }
            ExprKind::Unary { op, expr } => match op {
                UnOp::Neg => format!("(- {})", self.expr(expr)),
                UnOp::Not => format!("(not {})", self.expr(expr)),
                UnOp::Len => format!("(#{})", self.expr(expr)),
            },
            ExprKind::Paren(inner) => format!("({})", self.expr(inner)),
            ExprKind::Table(fields) => {
                let items: Vec<String> = fields
                    .iter()
                    .map(|f| match f {
                        TableField::Positional(v) => self.expr(v),
                        TableField::Named(k, v) => format!("{k} = {}", self.expr(v)),
                        TableField::Keyed(k, v) => format!("[{}] = {}", self.expr(k), self.expr(v)),
                    })
                    .collect();
                format!("{{{}}}", items.join(", "))
            }
        }
    }

    fn spec(&self, site: SiteId) -> String {
        let s = self.program().site(site);
        let args = self.args(&s.args);
        match self.a.targets[site as usize] {
            SiteTarget::Cpu => match &self.a.expansions[site as usize] {
                SiteExpansion::Cpu(_) => format!("__lg_cf[{}]({args})", site),
                _ => format!("__lg_gpu_only({})", lua_string(s.name.as_bytes())),
            },
            // PyFunction は実行時の環境に関数として置かれている(self は渡さない)。
            SiteTarget::Py => {
                if s.property {
                    s.name.clone()
                } else {
                    format!("{}({args})", s.name)
                }
            }
            SiteTarget::Gpu => format!("__lg_gpu_only({})", lua_string(s.name.as_bytes())),
        }
    }

    pub fn function(&self, f: FuncId, local_name: Option<&str>) -> String {
        let func = self.program().func(f);
        let mut params: Vec<String> = func.params.iter().map(|p| self.var(*p)).collect();
        if func.is_vararg {
            params.push("...".into());
        }
        let mut body = String::new();
        self.block(&func.body, &mut body);
        match local_name {
            Some(n) => format!("local function {n}({}) {body} end", params.join(", ")),
            None => format!("function({}) {body} end", params.join(", ")),
        }
    }

    pub fn block(&self, b: &Block, out: &mut String) {
        for s in &b.stmts {
            self.stmt(s, out);
        }
    }

    fn lvalue(&self, t: &LValue) -> String {
        match t {
            LValue::Local(v, _) => self.var(*v),
            LValue::Global(g, _) => g.clone(),
            LValue::Index { obj, key, .. } => match &key.kind {
                ExprKind::Str(k) if std::str::from_utf8(k).is_ok_and(is_lua_identifier) => {
                    format!("{}.{}", self.prefix(obj), String::from_utf8_lossy(k))
                }
                _ => format!("{}[{}]", self.prefix(obj), self.expr(key)),
            },
        }
    }

    /// 文を 1 行で出力する(後ろに空白を付ける)。
    pub fn stmt(&self, s: &Stmt, out: &mut String) {
        match &s.kind {
            StmtKind::Local { vars, exprs } => {
                let names: Vec<String> = vars.iter().map(|v| self.var(*v)).collect();
                if exprs.is_empty() {
                    let _ = write!(out, "local {}; ", names.join(", "));
                } else {
                    let _ = write!(out, "local {} = {}; ", names.join(", "), self.args(exprs));
                }
            }
            StmtKind::Assign { targets, exprs } => {
                let ts: Vec<String> = targets.iter().map(|t| self.lvalue(t)).collect();
                let _ = write!(out, "{} = {}; ", ts.join(", "), self.args(exprs));
            }
            StmtKind::Call(e) => {
                let _ = write!(out, "{}; ", self.call_stmt(e));
            }
            StmtKind::Do(b) => {
                out.push_str("do ");
                self.block(b, out);
                out.push_str("end ");
            }
            StmtKind::While { cond, body } => {
                let _ = write!(out, "while {} do ", self.expr(cond));
                self.block(body, out);
                out.push_str("end ");
            }
            StmtKind::Repeat { body, cond } => {
                out.push_str("repeat ");
                self.block(body, out);
                let _ = write!(out, "until {} ", self.expr(cond));
            }
            StmtKind::If { arms, else_block } => {
                for (i, (c, b)) in arms.iter().enumerate() {
                    let kw = if i == 0 { "if" } else { "elseif" };
                    let _ = write!(out, "{kw} {} then ", self.expr(c));
                    self.block(b, out);
                }
                if let Some(b) = else_block {
                    out.push_str("else ");
                    self.block(b, out);
                }
                out.push_str("end ");
            }
            StmtKind::NumFor {
                var,
                start,
                limit,
                step,
                body,
            } => {
                let step = step.as_ref().map(|e| format!(", {}", self.expr(e))).unwrap_or_default();
                let _ = write!(
                    out,
                    "for {} = {}, {}{step} do ",
                    self.var(*var),
                    self.expr(start),
                    self.expr(limit)
                );
                self.block(body, out);
                out.push_str("end ");
            }
            StmtKind::GenFor { vars, exprs, body } => {
                let names: Vec<String> = vars.iter().map(|v| self.var(*v)).collect();
                let _ = write!(out, "for {} in {} do ", names.join(", "), self.args(exprs));
                self.block(body, out);
                out.push_str("end ");
            }
            StmtKind::LocalFunction { var, func } => {
                let n = self.var(*var);
                let _ = write!(out, "{} ", self.function(*func, Some(&n)));
            }
            StmtKind::Return(exprs) => {
                // return はブロックの最後でなければならないので do ... end で包む。
                let _ = write!(out, "do return {} end ", self.args(exprs));
            }
            StmtKind::Break => out.push_str("do break end "),
            StmtKind::Goto(l) => {
                let _ = write!(out, "goto {l}; ");
            }
            StmtKind::Label(l) => {
                let _ = write!(out, "::{l}:: ");
            }
        }
    }

    /// 式文。Lua では呼び出しだけが文になれるので、それ以外(記録で包んだ値など)は代入で捨てる。
    pub fn call_stmt(&self, e: &Expr) -> String {
        let s = self.expr(e);
        match &e.kind {
            ExprKind::Call { .. } | ExprKind::Method { .. } | ExprKind::Spec(_) => s,
            _ => format!("local _ = {s}"),
        }
    }
}

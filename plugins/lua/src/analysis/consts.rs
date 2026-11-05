//! 生成関数に渡す CallSite の `consts` / `types` を求める前処理。
//!
//! 生成関数を呼ぶ前(生成コードがまだ無い段階)なので、簡単な定数伝播だけを行う:
//! リテラル、宣言時の初期値が定数で以後代入されない local、それらの単項・二項演算。

use std::collections::HashMap;

use crate::hir::*;
use crate::spec::ConstValue;

/// 各 site の引数の (定数値, 型名)。
#[derive(Clone, Debug, Default)]
pub struct SiteArgInfo {
    pub consts: Vec<Option<ConstValue>>,
    pub types: Vec<String>,
}

pub fn site_arg_info(program: &Program) -> Vec<SiteArgInfo> {
    let mut cx = Cx {
        program,
        local_consts: HashMap::new(),
    };
    // 宣言時の定数(以後代入されない local のみ)を、全関数を走査して集める。
    for f in &program.funcs {
        cx.collect_block(&f.body);
    }
    program
        .sites
        .iter()
        .map(|s| SiteArgInfo {
            consts: s.args.iter().map(|a| cx.const_of(a)).collect(),
            types: s.args.iter().map(|a| cx.type_of(a).to_string()).collect(),
        })
        .collect()
}

struct Cx<'a> {
    program: &'a Program,
    local_consts: HashMap<VarId, ConstValue>,
}

impl Cx<'_> {
    fn collect_block(&mut self, b: &Block) {
        for s in &b.stmts {
            match &s.kind {
                StmtKind::Local { vars, exprs } => {
                    for (i, v) in vars.iter().enumerate() {
                        if self.program.var(*v).reassigned {
                            continue;
                        }
                        // 最後の式が多値なら、それ以降の変数の値は分からない。
                        let Some(e) = exprs.get(i) else { break };
                        if i + 1 == exprs.len() && e.is_multi(self.program) && i + 1 < vars.len() {
                            break;
                        }
                        if let Some(c) = self.const_of(e) {
                            self.local_consts.insert(*v, c);
                        }
                    }
                }
                StmtKind::Do(b) => self.collect_block(b),
                StmtKind::While { body, .. } | StmtKind::Repeat { body, .. } => {
                    self.collect_block(body)
                }
                StmtKind::NumFor { body, .. } | StmtKind::GenFor { body, .. } => {
                    self.collect_block(body)
                }
                StmtKind::If { arms, else_block } => {
                    for (_, b) in arms {
                        self.collect_block(b);
                    }
                    if let Some(b) = else_block {
                        self.collect_block(b);
                    }
                }
                _ => {}
            }
        }
    }

    pub fn const_of(&self, e: &Expr) -> Option<ConstValue> {
        match &e.kind {
            ExprKind::Nil => Some(ConstValue::Nil),
            ExprKind::Bool(b) => Some(ConstValue::Bool(*b)),
            ExprKind::Number(n) => Some(ConstValue::Number(*n)),
            ExprKind::Str(s) => Some(ConstValue::String(String::from_utf8_lossy(s).into_owned())),
            ExprKind::Local(v) => self.local_consts.get(v).cloned(),
            ExprKind::Paren(e) => self.const_of(e),
            ExprKind::Unary { op, expr } => match (op, self.const_of(expr)?) {
                (UnOp::Neg, ConstValue::Number(n)) => Some(ConstValue::Number(-n)),
                (UnOp::Not, c) => Some(ConstValue::Bool(!truthy(&c))),
                (UnOp::Len, ConstValue::String(s)) => Some(ConstValue::Number(s.len() as f64)),
                _ => None,
            },
            ExprKind::Binary { op, lhs, rhs } => {
                let a = self.const_of(lhs)?;
                match op {
                    BinOp::And => return if truthy(&a) { self.const_of(rhs) } else { Some(a) },
                    BinOp::Or => return if truthy(&a) { Some(a) } else { self.const_of(rhs) },
                    _ => {}
                }
                let b = self.const_of(rhs)?;
                fold_binary(*op, &a, &b)
            }
            _ => None,
        }
    }

    fn type_of(&self, e: &Expr) -> &'static str {
        if let Some(c) = self.const_of(e) {
            return match c {
                ConstValue::Nil => "nil",
                ConstValue::Bool(_) => "bool",
                ConstValue::Number(n) if n.fract() == 0.0 && n.abs() < 2f64.powi(31) => "int",
                ConstValue::Number(_) => "float",
                ConstValue::String(_) => "string",
            };
        }
        match &e.kind {
            ExprKind::Binary { op, .. } if op.is_arith() => "float",
            ExprKind::Binary { op, .. } if op.is_compare() => "bool",
            ExprKind::Binary { op: BinOp::Concat, .. } => "string",
            ExprKind::Unary { op: UnOp::Neg, .. } | ExprKind::Unary { op: UnOp::Len, .. } => {
                "float"
            }
            ExprKind::Unary { op: UnOp::Not, .. } => "bool",
            ExprKind::Paren(e) => self.type_of(e),
            _ => "any",
        }
    }
}

pub fn truthy(c: &ConstValue) -> bool {
    !matches!(c, ConstValue::Nil | ConstValue::Bool(false))
}

/// 定数どうしの二項演算(Lua の意味論。文字列→数値の自動変換は扱わない)。
pub fn fold_binary(op: BinOp, a: &ConstValue, b: &ConstValue) -> Option<ConstValue> {
    use ConstValue::*;
    Some(match (op, a, b) {
        (BinOp::Add, Number(x), Number(y)) => Number(x + y),
        (BinOp::Sub, Number(x), Number(y)) => Number(x - y),
        (BinOp::Mul, Number(x), Number(y)) => Number(x * y),
        (BinOp::Div, Number(x), Number(y)) => Number(x / y),
        (BinOp::Mod, Number(x), Number(y)) => Number(x - (x / y).floor() * y),
        (BinOp::Pow, Number(x), Number(y)) => Number(x.powf(*y)),
        (BinOp::Concat, String(x), String(y)) => String(format!("{x}{y}")),
        (BinOp::Eq, x, y) => Bool(x == y),
        (BinOp::Ne, x, y) => Bool(x != y),
        (BinOp::Lt, Number(x), Number(y)) => Bool(x < y),
        (BinOp::Le, Number(x), Number(y)) => Bool(x <= y),
        (BinOp::Gt, Number(x), Number(y)) => Bool(x > y),
        (BinOp::Ge, Number(x), Number(y)) => Bool(x >= y),
        (BinOp::Lt, String(x), String(y)) => Bool(x < y),
        (BinOp::Le, String(x), String(y)) => Bool(x <= y),
        (BinOp::Gt, String(x), String(y)) => Bool(x > y),
        (BinOp::Ge, String(x), String(y)) => Bool(x >= y),
        _ => return None,
    })
}

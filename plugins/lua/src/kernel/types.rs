//! GPU 側(Slang)で扱う値の型。
//!
//! Lua の number はすべて `float`(f32)で表す(誤差は許容する、という決定事項)。
//! ループ変数や添字は Slang 側で int に変換して使うが、Lua の値としては float のまま。

use crate::hir::FuncId;
use crate::stdlib::Builtin;
use crate::types::TypeSpec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrLen {
    /// カーネル内で作る固定長(上限)の配列。実際の長さは別の変数で持つ。
    Local(u32),
    /// CPU から params で渡される実行時長の配列(読み取り専用)。
    Param,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GpuTy {
    Float,
    Bool,
    Array(Box<GpuTy>, ArrLen),
    Record(Vec<(String, GpuTy)>),
    /// 関数値(呼び出し先としてだけ使える)。
    Func(FuncId),
    /// 組み込みの関数・ライブラリ値(呼び出し先としてだけ使える)。
    Builtin(&'static Builtin),
    /// まだ推論されていない(使われ方から決める)。
    Unknown,
    /// GPU では表現できない値(文字列、nil、クロージャなど)。
    Cpu,
}

impl GpuTy {
    pub fn is_scalar(&self) -> bool {
        matches!(self, GpuTy::Float | GpuTy::Bool)
    }

    /// 変数・params として GPU に置ける型か。
    pub fn is_storable(&self) -> bool {
        match self {
            GpuTy::Float | GpuTy::Bool => true,
            GpuTy::Array(e, _) => e.is_storable(),
            GpuTy::Record(fields) => fields.iter().all(|(_, t)| t.is_storable()),
            _ => false,
        }
    }

    /// 2 つの型を合わせる。Unknown は相手に合わせる。合わなければ None。
    pub fn unify(&self, other: &GpuTy) -> Option<GpuTy> {
        use GpuTy::*;
        Some(match (self, other) {
            (Unknown, t) | (t, Unknown) => t.clone(),
            (Float, Float) => Float,
            (Bool, Bool) => Bool,
            (Array(a, la), Array(b, lb)) => {
                let len = match (la, lb) {
                    (ArrLen::Local(x), ArrLen::Local(y)) => ArrLen::Local((*x).max(*y)),
                    (ArrLen::Param, ArrLen::Param) => ArrLen::Param,
                    _ => return None,
                };
                Array(Box::new(a.unify(b)?), len)
            }
            (Record(a), Record(b)) => {
                let mut fields = a.clone();
                for (k, t) in b {
                    match fields.iter_mut().find(|(n, _)| n == k) {
                        Some((_, ft)) => *ft = ft.unify(t)?,
                        None => fields.push((k.clone(), t.clone())),
                    }
                }
                Record(fields)
            }
            (Func(a), Func(b)) if a == b => Func(*a),
            (Builtin(a), Builtin(b)) if a.name == b.name => Builtin(*a),
            _ => return None,
        })
    }

    pub fn from_spec(t: &TypeSpec) -> GpuTy {
        match t {
            TypeSpec::Float | TypeSpec::Int => GpuTy::Float,
            TypeSpec::Bool => GpuTy::Bool,
            TypeSpec::String | TypeSpec::Any => GpuTy::Cpu,
            TypeSpec::Array(e, _) => GpuTy::Array(Box::new(GpuTy::from_spec(e)), ArrLen::Param),
            TypeSpec::Record(fields) => GpuTy::Record(
                fields
                    .iter()
                    .map(|(k, v)| (k.clone(), GpuTy::from_spec(v)))
                    .collect(),
            ),
        }
    }

    /// 配列をすべて CPU から渡す実行時長の配列(Param)にする(捕捉する値の型に使う)。
    pub fn as_param(&self) -> GpuTy {
        match self {
            GpuTy::Array(e, _) => GpuTy::Array(Box::new(e.as_param()), ArrLen::Param),
            GpuTy::Record(f) => {
                GpuTy::Record(f.iter().map(|(k, t)| (k.clone(), t.as_param())).collect())
            }
            t => t.clone(),
        }
    }

    /// Unknown を既定の型(float)で埋める。
    pub fn defaulted(&self) -> GpuTy {
        match self {
            GpuTy::Unknown => GpuTy::Float,
            GpuTy::Array(e, l) => GpuTy::Array(Box::new(e.defaulted()), *l),
            GpuTy::Record(f) => {
                GpuTy::Record(f.iter().map(|(k, t)| (k.clone(), t.defaulted())).collect())
            }
            t => t.clone(),
        }
    }

    /// 表示用(診断・レイアウトヘッダー)。
    pub fn describe(&self) -> String {
        match self {
            GpuTy::Float => "float".into(),
            GpuTy::Bool => "bool".into(),
            GpuTy::Array(e, ArrLen::Param) => format!("{}[]", e.describe()),
            GpuTy::Array(e, ArrLen::Local(n)) => format!("{}[{n}]", e.describe()),
            GpuTy::Record(f) => format!(
                "{{{}}}",
                f.iter()
                    .map(|(k, t)| format!("{k}: {}", t.describe()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            GpuTy::Func(_) => "function".into(),
            GpuTy::Builtin(b) => b.name.into(),
            GpuTy::Unknown => "unknown".into(),
            GpuTy::Cpu => "cpu-only".into(),
        }
    }
}

/// GPU に変換するユーザー関数のシグネチャ(1 関数につき 1 つの特殊化)。
#[derive(Clone, Debug, PartialEq)]
pub struct FuncSig {
    pub params: Vec<GpuTy>,
    pub returns: Vec<GpuTy>,
}

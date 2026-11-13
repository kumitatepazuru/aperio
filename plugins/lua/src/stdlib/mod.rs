//! LuaJIT 2.1 の標準ライブラリの分類表。
//!
//! 意味が問題になるのはカーネル(GPU)の中だけで、カーネルの外は LuaJIT がそのまま実行する。
//! 各関数について:
//! - [`Class`]: A = Slang の組み込みへ 1 対 1、B = ヘルパーの埋め込み / 静的解決、C = CPU 退避必須
//! - [`Effect`]: カーネル内で CPU に退避するときの扱い(持ち出し / 再生 / エラー)
//! - [`GpuImpl`]: GPU での実装方法
//!
//! 表の網羅性は、実際の LuaJIT ランタイムのグローバル / ライブラリテーブルを列挙するテスト
//! (`tests/stdlib_coverage.rs`)で検査する。

mod base;
mod bit;
mod coroutine;
mod debug;
mod ffi;
mod io;
mod jit;
mod math;
mod os;
mod package;
mod string;
mod table;

use std::collections::HashMap;
use std::sync::OnceLock;

/// 分類(計画書 §7 の A / B / C)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Slang の組み込みへ 1 対 1 で置き換え。
    A,
    /// ヘルパー関数の埋め込み、または変換時の静的解決で置き換え。
    B,
    /// CPU 退避が必須。
    C,
}

/// カーネル内での副作用の種類。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Effect {
    /// 副作用なし。
    Pure,
    /// 1 回の実行の中では冪等な読み取り(ループ不変なら持ち出してよい)。
    ReadOnly,
    /// 呼び出し順に意味がある状態を持つ(乱数・逐次読み取り)。
    Stream,
    /// 外から観測できる副作用。
    Write,
    /// error / assert。
    Control,
    /// 中身が解析できない(load / coroutine / debug など)。
    Opaque,
}

impl Effect {
    pub fn join(self, other: Effect) -> Effect {
        self.max(other)
    }
}

/// GPU での実装方法。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GpuImpl {
    /// Slang の組み込み関数(同じ引数順)。
    Intrinsic(&'static str),
    /// helpers.slang の関数。
    Helper(&'static str),
    /// 変換器が個別に扱う(type / select / unpack / ipairs など)。
    Special,
    /// 数値定数。
    Const(f64),
    /// GPU で実装できない。
    None,
}

/// 値の種類(関数か、定数などの値か)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Function,
    /// 関数ではない値(math.pi、io.stdout、package.path など)。
    Value,
    /// ライブラリテーブル自体(math、string など)。
    Library,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Builtin {
    /// `math.sin` / `print` / `file:read` のような名前。
    pub name: &'static str,
    pub kind: Kind,
    pub class: Class,
    pub effect: Effect,
    pub gpu: GpuImpl,
    /// 意味論のメモ(LuaJIT のソースで確認した挙動など)。
    pub note: &'static str,
}

pub(crate) const fn f(
    name: &'static str,
    class: Class,
    effect: Effect,
    gpu: GpuImpl,
    note: &'static str,
) -> Builtin {
    Builtin {
        name,
        kind: Kind::Function,
        class,
        effect,
        gpu,
        note,
    }
}

pub(crate) const fn v(
    name: &'static str,
    class: Class,
    effect: Effect,
    gpu: GpuImpl,
    note: &'static str,
) -> Builtin {
    Builtin {
        name,
        kind: Kind::Value,
        class,
        effect,
        gpu,
        note,
    }
}

pub(crate) const fn lib(name: &'static str) -> Builtin {
    Builtin {
        name,
        kind: Kind::Library,
        class: Class::C,
        effect: Effect::Pure,
        gpu: GpuImpl::None,
        note: "library table",
    }
}

/// すべての組み込み(ライブラリ順)。
pub fn all() -> impl Iterator<Item = &'static Builtin> {
    [
        base::ENTRIES,
        math::ENTRIES,
        string::ENTRIES,
        table::ENTRIES,
        io::ENTRIES,
        os::ENTRIES,
        package::ENTRIES,
        coroutine::ENTRIES,
        debug::ENTRIES,
        bit::ENTRIES,
        jit::ENTRIES,
        ffi::ENTRIES,
    ]
    .into_iter()
    .flatten()
}

fn index() -> &'static HashMap<&'static str, &'static Builtin> {
    static INDEX: OnceLock<HashMap<&'static str, &'static Builtin>> = OnceLock::new();
    INDEX.get_or_init(|| all().map(|b| (b.name, b)).collect())
}

/// 名前(`math.sin` / `print` / `file:read`)で引く。
pub fn lookup(name: &str) -> Option<&'static Builtin> {
    index().get(name).copied()
}

/// グローバル変数として存在する名前か(`math`, `print` など)。
pub fn is_global(name: &str) -> bool {
    lookup(name).is_some()
}

//! ランタイムのエラー型。

use std::error::Error as StdError;
use std::fmt;

use super::callback::CallbackError;

#[derive(Debug)]
pub enum Error {
    /// 値が宣言型と合わない。
    Type(String),
    /// 登録されていない変数。
    UnknownVariable(String),
    /// 名前が不正、または登録済み・途中がテーブルでないなどで使えない。
    InvalidName(String),
    /// Lua の実行・コンパイルエラー。
    Lua(mlua::Error),
    /// [`Callback`](super::Callback) が返したエラー(元のエラーをそのまま保持する)。
    Callback(CallbackError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Type(m) => write!(f, "{m}"),
            Error::UnknownVariable(name) => write!(f, "unknown variable: '{name}'"),
            Error::InvalidName(m) => write!(f, "{m}"),
            Error::Lua(e) => write!(f, "{e}"),
            Error::Callback(e) => write!(f, "{e}"),
        }
    }
}

impl StdError for Error {}

impl From<mlua::Error> for Error {
    fn from(e: mlua::Error) -> Self {
        Error::Lua(e)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

//! Lua(LuaJIT)スクリプトランタイム
//!
//! コア部分は pyo3 に依存せず、`python` feature を有効にしたときだけ Python バインディングを公開する。

#[cfg(feature = "python")]
mod python;
pub mod runtime;

pub use runtime::{Callback, CallbackError, Error, LuaRuntime, Value, ValueType};

/// クレートのバージョン(Cargo.toml の version)。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

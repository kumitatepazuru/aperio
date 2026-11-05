//! Lua(LuaJIT)スクリプトを「CPU で 1 回実行する Lua」と「GPU で並列実行する Slang カーネル」に
//! 分割・変換するライブラリ。
//!
//! コア部分は pyo3 に依存せず、`python` feature を有効にしたときだけ Python バインディングを公開する。

#[cfg(feature = "python")]
mod python;

/// クレートのバージョン(Cargo.toml の version)。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

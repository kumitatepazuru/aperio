//! Lua(LuaJIT)スクリプトを「CPU で 1 回実行する Lua」と「GPU で並列実行する Slang カーネル」に
//! 分割・変換するライブラリ。
//!
//! コア部分は pyo3 に依存せず、`python` feature を有効にしたときだけ Python バインディングを公開する。

pub mod analysis;
pub mod codegen;
pub mod diag;
pub mod expand;
pub mod frontend;
pub mod hir;
pub mod kernel;
pub mod pipeline;
pub mod runtime;
pub mod spec;
pub mod stdlib;
pub mod types;

#[cfg(feature = "python")]
mod python;

use std::path::{Path, PathBuf};

use crate::codegen::layout::KernelLayout;
use crate::diag::{Diagnostic, Diagnostics};
use crate::spec::Registry;
use crate::types::TypeSpec;

/// クレートのバージョン(Cargo.toml の version)。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 変換結果。
#[derive(Clone, Debug)]
pub struct Converted {
    /// CPU で実行する Lua(1 行目にレイアウトのヘッダー)。
    pub lua: String,
    /// カーネルごとの Slang ソース(ディスパッチの k が添字)。
    pub shaders: Vec<String>,
    /// run に渡す外部値の一覧(宣言順)。
    pub externals: Vec<(String, TypeSpec)>,
    pub layouts: Vec<KernelLayout>,
}

#[derive(Clone, Debug)]
pub struct ConvertOutput {
    /// 失敗したら None(理由は diagnostics)。
    pub result: Option<Converted>,
    pub diagnostics: Vec<Diagnostic>,
}

/// full-moon のパーサーは再帰下降なので、深いネストに備えて大きなスタックで実行する。
const CONVERT_STACK: usize = 16 * 1024 * 1024;

/// Lua のソースを変換する。
pub fn convert(
    source: &str,
    chunk_name: &str,
    base_dir: Option<&Path>,
    registry: &Registry,
) -> ConvertOutput {
    let source = source.to_string();
    let chunk = chunk_name.to_string();
    let base: Option<PathBuf> = base_dir.map(Path::to_path_buf);
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .name("lua_gpu-convert".into())
            .stack_size(CONVERT_STACK)
            .spawn_scoped(s, || convert_inner(&source, &chunk, base.as_deref(), registry))
            .expect("failed to spawn the convert thread")
            .join()
            .unwrap_or_else(|_| ConvertOutput {
                result: None,
                diagnostics: vec![Diagnostic {
                    severity: diag::Severity::Error,
                    code: "internal",
                    message: "the converter panicked".into(),
                    location: diag::Location::new(chunk_name, 0, 0),
                }],
            })
    })
}

fn convert_inner(
    source: &str,
    chunk_name: &str,
    base_dir: Option<&Path>,
    registry: &Registry,
) -> ConvertOutput {
    let mut diags = Diagnostics::new();
    let result = (|| {
        let analysis = pipeline::analyze(source, chunk_name, base_dir, registry, &mut diags)?;
        if diags.has_errors() {
            return None;
        }
        let mut slangs = Vec::new();
        for k in &analysis.plan.kernels {
            slangs.push(codegen::slang::generate(&analysis, k, registry, &mut diags)?);
        }
        let pairs: Vec<_> = analysis.plan.kernels.iter().zip(slangs.iter()).collect();
        let lua = codegen::cpu_rewrite::generate(&analysis, &pairs, &mut diags)?;
        if diags.has_errors() {
            return None;
        }
        Some(Converted {
            lua,
            layouts: slangs.iter().map(|s| s.layout.clone()).collect(),
            shaders: slangs.into_iter().map(|s| s.source).collect(),
            externals: registry
                .externals
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        })
    })();
    ConvertOutput {
        result,
        diagnostics: diags.into_items(),
    }
}

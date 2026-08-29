// compose_wgsl.rs
//
// naga_oil を使った WGSL の結合。
// naga_oil の `ComposableModuleDescriptor` / `NagaModuleDescriptor` は
// `source` / `file_path` を `&str` で借用するため、ファイルから読み込んだソースを
// そのまま descriptor として持ち回ることができない。
// ここではソースを所有する構造体を用意し、合成時に descriptor を組み立てる。

use std::collections::HashMap;

use anyhow::{Context, Result};
use naga_oil::compose::{ComposableModuleDescriptor, NagaModuleDescriptor, ShaderDefValue};

/// `#import` される側のシェーダー。
///
/// モジュール名はソース中の `#define_import_path` から naga_oil が読み取る。
pub struct ComposableModuleSource {
    pub file_path: String,
    pub source: String,
    pub shader_defs: HashMap<String, ShaderDefValue>,
}

impl ComposableModuleSource {
    pub fn descriptor(&self) -> ComposableModuleDescriptor<'_> {
        ComposableModuleDescriptor {
            source: &self.source,
            file_path: &self.file_path,
            shader_defs: self.shader_defs.clone(),
            // language は ShaderLanguage::Wgsl、as_name は None、additional_imports は空がデフォルト
            ..Default::default()
        }
    }
}

/// 最終的に naga module へ変換されるエントリシェーダー。
pub struct NagaModuleSource {
    pub file_path: String,
    pub source: String,
    pub shader_defs: HashMap<String, ShaderDefValue>,
}

impl NagaModuleSource {
    pub fn descriptor(&self) -> NagaModuleDescriptor<'_> {
        NagaModuleDescriptor {
            source: &self.source,
            file_path: &self.file_path,
            shader_defs: self.shader_defs.clone(),
            // shader_type は ShaderType::Wgsl、additional_imports は空がデフォルト
            ..Default::default()
        }
    }
}

/// `file_path` のシェーダーを読み込み、composable module として登録できる形にする。
pub fn create_composable_module(
    file_path: &str,
    shader_defs: HashMap<String, ShaderDefValue>,
) -> Result<ComposableModuleSource> {
    let source = std::fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read composable module: {file_path}"))?;

    Ok(ComposableModuleSource {
        file_path: file_path.to_string(),
        source,
        shader_defs,
    })
}

/// `file_path` のシェーダーを読み込み、エントリシェーダーとして扱える形にする。
pub fn create_naga_module(
    file_path: &str,
    shader_defs: HashMap<String, ShaderDefValue>,
) -> Result<NagaModuleSource> {
    let source = std::fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read naga module: {file_path}"))?;

    Ok(NagaModuleSource {
        file_path: file_path.to_string(),
        source,
        shader_defs,
    })
}

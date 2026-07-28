use std::collections::HashMap;

use naga_oil::compose::ShaderDefValue;
use pyo3::{exceptions::PyValueError, prelude::*, types::PyDict};

use gpu_util::compose_wgsl;

/// `#import` される側のシェーダー。`ComposableModuleDescriptor` の素になる。
#[pyclass]
pub struct PyComposableModuleDescriptor {
    pub inner: compose_wgsl::ComposableModuleSource,
}

/// 最終的に naga module へ変換されるエントリシェーダー。`NagaModuleDescriptor` の素になる。
#[pyclass]
pub struct PyNagaModuleDescriptor {
    pub inner: compose_wgsl::NagaModuleSource,
}

/// Python の dict を naga_oil の shader_defs に変換する。
/// `ShaderDefValue` は Bool / Int / UInt しか持たないため、float は受け付けない。
fn to_shader_defs(
    shader_defs: Option<&Bound<'_, PyDict>>,
) -> PyResult<HashMap<String, ShaderDefValue>> {
    let Some(shader_defs) = shader_defs else {
        return Ok(HashMap::new());
    };

    let mut result = HashMap::with_capacity(shader_defs.len());
    for (key, value) in shader_defs.iter() {
        let key: String = key.extract()?;

        // bool は int にも extract できてしまうため、必ず bool を先に判定する
        let value = if let Ok(v) = value.extract::<bool>() {
            ShaderDefValue::Bool(v)
        } else if let Ok(v) = value.extract::<i32>() {
            ShaderDefValue::Int(v)
        } else if let Ok(v) = value.extract::<u32>() {
            ShaderDefValue::UInt(v)
        } else {
            return Err(PyValueError::new_err(format!(
                "Invalid shader_def value for {key:?}. Must be bool or int"
            )));
        };

        result.insert(key, value);
    }

    Ok(result)
}

/// `file_path` のシェーダーを読み込み、composable module として登録できる形にする。
#[pyfunction]
#[pyo3(signature = (file_path, shader_defs=None))]
pub fn create_composable_module(
    file_path: &str,
    shader_defs: Option<&Bound<'_, PyDict>>,
) -> PyResult<PyComposableModuleDescriptor> {
    let inner = compose_wgsl::create_composable_module(file_path, to_shader_defs(shader_defs)?)?;

    Ok(PyComposableModuleDescriptor { inner })
}

/// `file_path` のシェーダーを読み込み、エントリシェーダーとして扱える形にする。
#[pyfunction]
#[pyo3(signature = (file_path, shader_defs=None))]
pub fn create_naga_module(
    file_path: &str,
    shader_defs: Option<&Bound<'_, PyDict>>,
) -> PyResult<PyNagaModuleDescriptor> {
    let inner = compose_wgsl::create_naga_module(file_path, to_shader_defs(shader_defs)?)?;

    Ok(PyNagaModuleDescriptor { inner })
}

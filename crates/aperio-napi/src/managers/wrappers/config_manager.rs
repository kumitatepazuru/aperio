use std::sync::{Arc, Mutex};

use napi_derive::napi;
use pyo3::prelude::*;

use crate::managers::config_manager::{AperioConfig, ConfigManager};

// ── napi wrapper ──────────────────────────────────────────────────────────

#[napi(js_name = "ConfigManager")]
pub struct NapiConfigManager {
    inner: Arc<Mutex<ConfigManager>>,
}

// 一部IDEでanalyserが誤ってエラーを出すため挿入
//noinspection RsCompileErrorMacro
#[napi]
impl NapiConfigManager {
    #[napi(getter, js_name = "config")]
    pub fn get_config(&self) -> napi::Result<AperioConfig> {
        let inner = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(inner.get_config())
    }

    #[napi]
    pub fn set_config(&self, config: AperioConfig) -> napi::Result<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        inner
            .set_config(config)
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }
}

impl NapiConfigManager {
    pub fn new(inner: Arc<Mutex<ConfigManager>>) -> Self {
        Self { inner }
    }
}

// ── pyo3 wrapper ──────────────────────────────────────────────────────────

#[pyclass(unsendable, name = "ConfigManager")]
pub struct PyConfigManager {
    inner: Arc<Mutex<ConfigManager>>,
}

#[pymethods]
impl PyConfigManager {
    #[getter]
    pub fn config(&self) -> PyResult<AperioConfig> {
        let inner = self
            .inner
            .lock()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(inner.get_config())
    }

    #[setter]
    pub fn set_config(&self, config: AperioConfig) -> PyResult<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        inner
            .set_config(config)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }
}

impl PyConfigManager {
    pub fn new(inner: Arc<Mutex<ConfigManager>>) -> Self {
        Self { inner }
    }
}

// ── pymodule ──────────────────────────────────────────────────────────────

#[pymodule]
pub mod config_manager {
    #[pymodule_export]
    use crate::managers::config_manager::AperioConfig;
    #[pymodule_export]
    use super::PyConfigManager;
}

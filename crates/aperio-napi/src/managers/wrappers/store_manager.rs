use std::sync::{Arc, RwLock};

use napi_derive::napi;
use pyo3::prelude::*;

use crate::managers::store_manager::{
    StoreManager, SyncableState, SyncableStatePartial,
};

// ── napi wrapper ──────────────────────────────────────────────────────────

#[napi(js_name = "StoreManager")]
pub struct NapiStoreManager {
    inner: Arc<RwLock<StoreManager>>,
}

// 一部IDEでanalyserが誤ってエラーを出すため挿入
//noinspection RsCompileErrorMacro
#[napi]
impl NapiStoreManager {
    #[napi]
    pub fn get_state(&self) -> napi::Result<SyncableState> {
        let inner = self
            .inner
            .read()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        inner
            .get_state()
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }

    #[napi]
    pub fn set_partial(&self, partial: SyncableStatePartial) -> napi::Result<()> {
        let mut inner = self
            .inner
            .write()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        inner
            .set_partial(partial)
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }
}

impl NapiStoreManager {
    pub fn new(inner: Arc<RwLock<StoreManager>>) -> Self {
        Self { inner }
    }
}

// ── pyo3 wrapper ──────────────────────────────────────────────────────────

#[pyclass(unsendable, name = "StoreManager")]
pub struct PyStoreManager {
    inner: Arc<RwLock<StoreManager>>,
}

#[pymethods]
impl PyStoreManager {
    pub fn get_state(&self) -> PyResult<SyncableState> {
        let inner = self
            .inner
            .read()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        inner
            .get_state()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    pub fn set_partial(&self, partial: SyncableStatePartial) -> PyResult<()> {
        let mut inner = self
            .inner
            .write()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        inner
            .set_partial(partial)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }
}

impl PyStoreManager {
    pub fn new(inner: Arc<RwLock<StoreManager>>) -> Self {
        Self { inner }
    }
}

// ── pymodule ──────────────────────────────────────────────────────────────

#[pymodule]
pub mod store {
    #[pymodule_export]
    use super::PyStoreManager;
    #[pymodule_export]
    use crate::managers::store_manager::ColorPickerState;
    #[pymodule_export]
    use crate::managers::store_manager::FrameState;
    #[pymodule_export]
    use crate::managers::store_manager::SyncableState;
    #[pymodule_export]
    use crate::managers::store_manager::SyncableStatePartial;
    #[pymodule_export]
    use crate::managers::store_manager::ViewerState;
}

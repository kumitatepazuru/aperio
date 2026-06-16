use std::sync::{Arc, Mutex};

use napi_derive::napi;
use pyo3::prelude::*;

use crate::managers::audio_manager::AudioManager;

// ── napi wrapper ──────────────────────────────────────────────────────────

#[napi(js_name = "AudioManager")]
pub struct NapiAudioManager {
    inner: Arc<Mutex<AudioManager>>,
}

// 一部IDEでanalyserが誤ってエラーを出すため挿入
//noinspection RsCompileErrorMacro
#[napi]
impl NapiAudioManager {
    #[napi(getter)]
    pub fn channels(&self) -> napi::Result<u16> {
        let inner = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(inner.channels())
    }

    #[napi(setter)]
    pub fn set_channels(&self, value: u16) -> napi::Result<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        inner
            .set_channels(value)
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }

    #[napi(getter)]
    pub fn sample_rate(&self) -> napi::Result<u32> {
        let inner = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(inner.sample_rate())
    }

    #[napi(setter)]
    pub fn set_sample_rate(&self, value: u32) -> napi::Result<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        inner
            .set_sample_rate(value)
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }

    #[napi(getter)]
    pub fn bit_depth(&self) -> napi::Result<u32> {
        let inner = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(inner.bit_depth())
    }

    #[napi(setter)]
    pub fn set_bit_depth(&self, value: u32) -> napi::Result<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        inner.set_bit_depth(value);
        Ok(())
    }

    #[napi]
    pub fn stop(&self) -> napi::Result<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        inner
            .stop()
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }

    #[napi(getter)]
    pub fn current_time(&self) -> napi::Result<f64> {
        let inner = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        inner
            .current_time()
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }
}

impl NapiAudioManager {
    pub fn new(inner: Arc<Mutex<AudioManager>>) -> Self {
        Self { inner }
    }
}

// ── pyo3 wrapper ──────────────────────────────────────────────────────────

#[pyclass(unsendable, name = "AudioManager")]
pub struct PyAudioManager {
    inner: Arc<Mutex<AudioManager>>,
}

#[pymethods]
impl PyAudioManager {
    #[getter]
    pub fn channels(&self) -> PyResult<u16> {
        let inner = self
            .inner
            .lock()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(inner.channels())
    }

    #[setter]
    pub fn set_channels(&self, value: u16) -> PyResult<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        inner
            .set_channels(value)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[getter]
    pub fn sample_rate(&self) -> PyResult<u32> {
        let inner = self
            .inner
            .lock()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(inner.sample_rate())
    }

    #[setter]
    pub fn set_sample_rate(&self, value: u32) -> PyResult<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        inner
            .set_sample_rate(value)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[getter]
    pub fn bit_depth(&self) -> PyResult<u32> {
        let inner = self
            .inner
            .lock()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(inner.bit_depth())
    }

    #[setter]
    pub fn set_bit_depth(&self, value: u32) -> PyResult<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        inner.set_bit_depth(value);
        Ok(())
    }

    pub fn stack_audio(&self, data: Vec<Vec<f32>>, start_time: f64) -> PyResult<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        inner
            .stack_audio(data, start_time)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    pub fn stop(&self) -> PyResult<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        inner
            .stop()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[getter]
    pub fn current_time(&self) -> PyResult<f64> {
        let inner = self
            .inner
            .lock()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        inner
            .current_time()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }
}

impl PyAudioManager {
    pub fn new(inner: Arc<Mutex<AudioManager>>) -> Self {
        Self { inner }
    }
}

// ── pymodule ──────────────────────────────────────────────────────────────

#[pymodule(name = "audio")]
pub mod audio_register {
    #[pymodule_export]
    use super::PyAudioManager;
}

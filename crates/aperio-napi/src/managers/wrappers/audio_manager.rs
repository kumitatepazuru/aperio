use std::sync::{Arc, Mutex};

use napi_derive::napi;
use numpy::PyReadonlyArray2;
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

    #[napi(getter)]
    pub fn pending_samples(&self) -> napi::Result<u32> {
        let inner = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        inner
            .pending_samples()
            .map(|v| v as u32)
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

    /// `data`: 2-D numpy array of shape `(channels, samples)`, dtype float32.
    /// Accepts C-contiguous, Fortran-contiguous, or arbitrarily strided arrays.
    pub fn stack_audio(&self, data: PyReadonlyArray2<f32>, start_time: u64) -> PyResult<()> {
        let array = data.as_array();
        let n_chs = array.nrows();
        let n_samples = array.ncols();

        if n_chs == 0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "data must have at least one channel",
            ));
        }

        // Build interleaved buffer with the most cache-friendly path available.
        let interleaved = if array.is_standard_layout() {
            // C-contiguous layout: memory is [ch0_s0, ch0_s1, ..., ch1_s0, ch1_s1, ...]
            // Transpose to interleaved [ch0_s0, ch1_s0, ..., ch0_s1, ch1_s1, ...].
            let flat = array.as_slice().unwrap();
            let mut v = Vec::with_capacity(n_chs * n_samples);
            for s in 0..n_samples {
                for ch in 0..n_chs {
                    v.push(flat[ch * n_samples + s]);
                }
            }
            v
        } else if array
            .as_slice_memory_order()
            .is_some_and(|_| array.t().is_standard_layout())
        {
            // F-contiguous layout: memory is already interleaved [ch0_s0, ch1_s0, ..., ch0_s1, ...]
            array.as_slice_memory_order().unwrap().to_vec()
        } else {
            // General strided case: iterate column-by-column (one column = one sample frame).
            let mut v = Vec::with_capacity(n_chs * n_samples);
            for col in array.columns() {
                if let Some(s) = col.as_slice() {
                    v.extend_from_slice(s);
                } else {
                    v.extend(col.iter().copied());
                }
            }
            v
        };

        let mut inner = self
            .inner
            .lock()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        inner
            .stack_audio(interleaved, n_chs, start_time)
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

    #[getter]
    pub fn pending_samples(&self) -> PyResult<u64> {
        let inner = self
            .inner
            .lock()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        inner
            .pending_samples()
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

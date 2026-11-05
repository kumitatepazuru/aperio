//! Python バインディング(`python` feature)。

use pyo3::prelude::*;

/// Lua → Slang 変換器。
#[pyclass(name = "LuaGPU", module = "lua_gpu")]
pub struct PyLuaGpu {}

#[pymethods]
impl PyLuaGpu {
    #[new]
    fn new() -> Self {
        Self {}
    }
}

#[pymodule(name = "lua_gpu")]
pub mod lua_gpu_module {
    #[pymodule_export]
    use super::PyLuaGpu;

    #[pymodule_init]
    fn init(m: &pyo3::Bound<'_, pyo3::types::PyModule>) -> pyo3::PyResult<()> {
        use pyo3::types::PyModuleMethods;
        m.add("__version__", crate::VERSION)?;
        Ok(())
    }
}

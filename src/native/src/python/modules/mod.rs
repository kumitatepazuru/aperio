use pyo3::{prelude::*, types::PyDict, wrap_pymodule};
use crate::structs::*;

pub mod avloader;
pub mod gpu_util;
pub mod logger;
pub mod text_rendering;


#[pymodule]
mod aperio {
    #[pymodule_export]
    use super::avloader::avloader_register;
    #[pymodule_export]
    use super::frame_structure::frame_structure;
    #[pymodule_export]
    use super::gpu_util::gpu_util_register;
    #[pymodule_export]
    use super::logger::logger;
    #[pymodule_export]
    use super::text_rendering::text_rendering_register;
    #[pymodule_export]
    use crate::store::store;
}

/// `native` クレートが Python の sys.modules にモジュールを注入する際に使う関数。
/// aperio / aperio.gpu_util / aperio.logger / aperio.frame_structure をすべて
/// sys.modules に登録する。
pub fn register_all(py: Python<'_>, sys_modules: &Bound<'_, PyDict>) -> PyResult<()> {
    let aperio_mod = PyModule::new(py, "aperio")?;
    aperio_mod.add_wrapped(wrap_pymodule!(aperio))?;

    sys_modules.set_item("aperio", aperio_mod)?;
    Ok(())
}

use crate::structs::*;
use aperio_derive::register_submodules;
use pyo3::{prelude::*, types::PyDict};

pub mod avloader;
pub mod gpu_util;
pub mod logger;
pub mod text_rendering;

#[register_submodules]
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
    // macroによって生成された隠しモジュールを使用して、サブモジュールも含めてすべて登録する
    sys_modules.set_item("aperio", aperio::_PYO3_DEF.make_module(py)?)?;
    Ok(())
}

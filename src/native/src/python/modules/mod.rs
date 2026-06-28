use crate::{
    managers::wrappers::{
        audio_manager::PyAudioManager, config_manager::PyConfigManager,
        store_manager::PyStoreManager,
    },
    structs::*,
};
use aperio_derive::register_submodules;
use pyo3::{prelude::*, types::PyDict};

pub mod avloader;
pub mod gpu_util;
pub mod logger;
pub mod text_rendering;

#[pyclass(get_all)]
pub struct PyManagers {
    pub config_manager: Py<PyConfigManager>,
    pub store_manager: Py<PyStoreManager>,
    pub audio_manager: Py<PyAudioManager>,
}

#[register_submodules]
#[pymodule]
mod aperio {
    #[pymodule_export]
    use super::avloader::avloader_register;
    #[pymodule_export]
    use super::gpu_util::gpu_util_register;
    #[pymodule_export]
    use super::item_structures::item_structures;
    #[pymodule_export]
    use super::logger::logger;
    #[pymodule_export]
    use super::text_rendering::text_rendering_register;
    #[pymodule_export]
    use super::PyManagers;
    #[pymodule_export]
    use crate::managers::wrappers::audio_manager::audio_register;
    #[pymodule_export]
    use crate::managers::wrappers::config_manager::config_manager;
    #[pymodule_export]
    use crate::managers::wrappers::store_manager::store;
}

/// `native` クレートが Python の sys.modules にモジュールを注入する際に使う関数。
/// aperio / aperio.gpu_util / aperio.logger / aperio.item_structures をすべて
/// sys.modules に登録する。
pub fn register_all(py: Python<'_>, sys_modules: &Bound<'_, PyDict>) -> PyResult<()> {
    // macroによって生成された隠しモジュールを使用して、サブモジュールも含めてすべて登録する
    sys_modules.set_item("aperio", aperio::_PYO3_DEF.make_module(py)?)?;
    Ok(())
}

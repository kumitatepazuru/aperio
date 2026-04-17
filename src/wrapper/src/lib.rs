use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use pyo3_stub_gen::define_stub_info_gatherer;

pub mod avloader;
pub mod frame_structure;
pub mod gpu_util;
pub mod logger;
pub mod text_rendering;
pub mod utils;

/// maturin でスタンドアロンビルドする場合のエントリポイント
#[pymodule]
pub mod aperio {
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
}

/// `native` クレートが Python の sys.modules にモジュールを注入する際に使う関数。
/// aperio / aperio.gpu_util / aperio.logger / aperio.frame_structure をすべて
/// sys.modules に登録する。
pub fn register_all(py: Python<'_>, sys_modules: &Bound<'_, PyDict>) -> PyResult<()> {
    let aperio_mod = PyModule::new(py, "aperio")?;

    for (name, init) in [
        (
            "gpu_util",
            gpu_util::gpu_util_register as fn(&Bound<'_, PyModule>) -> PyResult<()>,
        ),
        ("logger", logger::logger),
        ("frame_structure", frame_structure::frame_structure),
        ("text_rendering", text_rendering::text_rendering_register),
        ("avloader", avloader::avloader_register),
    ] {
        let sub = PyModule::new(py, &format!("aperio.{name}"))?;
        init(&sub)?;
        aperio_mod.add(name, &sub)?;
        sys_modules.set_item(format!("aperio.{name}"), sub)?;
    }

    sys_modules.set_item("aperio", aperio_mod)?;
    Ok(())
}

define_stub_info_gatherer!(stub_info);

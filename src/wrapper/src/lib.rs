use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use pyo3_stub_gen::define_stub_info_gatherer;

pub mod frame_structure;
pub mod gpu_util;
pub mod logger;
pub mod utils;

/// maturin でスタンドアロンビルドする場合のエントリポイント
#[pymodule]
pub fn aperio(m: &Bound<PyModule>) -> PyResult<()> {
    let py = m.py();

    let gpu_util_mod = PyModule::new(py, "aperio.gpu_util")?;
    gpu_util::register(&gpu_util_mod)?;
    m.add_submodule(&gpu_util_mod)?;

    let logger_mod = PyModule::new(py, "aperio.logger")?;
    logger::register(&logger_mod)?;
    m.add_submodule(&logger_mod)?;

    let frame_structure_mod = PyModule::new(py, "aperio.frame_structure")?;
    frame_structure::register(&frame_structure_mod)?;
    m.add_submodule(&frame_structure_mod)?;

    Ok(())
}

/// `native` クレートが Python の sys.modules にモジュールを注入する際に使う関数。
/// aperio / aperio.gpu_util / aperio.logger / aperio.frame_structure をすべて
/// sys.modules に登録する。
pub fn register_all(py: Python<'_>, sys_modules: &Bound<'_, PyDict>) -> PyResult<()> {
    let aperio_mod = PyModule::new(py, "aperio")?;

    let gpu_util_mod = PyModule::new(py, "aperio.gpu_util")?;
    gpu_util::register(&gpu_util_mod)?;
    aperio_mod.add("gpu_util", &gpu_util_mod)?;
    sys_modules.set_item("aperio.gpu_util", &gpu_util_mod)?;

    let logger_mod = PyModule::new(py, "aperio.logger")?;
    logger::register(&logger_mod)?;
    aperio_mod.add("logger", &logger_mod)?;
    sys_modules.set_item("aperio.logger", &logger_mod)?;

    let frame_structure_mod = PyModule::new(py, "aperio.frame_structure")?;
    frame_structure::register(&frame_structure_mod)?;
    aperio_mod.add("frame_structure", &frame_structure_mod)?;
    sys_modules.set_item("aperio.frame_structure", &frame_structure_mod)?;

    sys_modules.set_item("aperio", &aperio_mod)?;

    Ok(())
}

define_stub_info_gatherer!(stub_info);

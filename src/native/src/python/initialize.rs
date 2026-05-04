use crate::util::get_data_dir;
use crate::Dirs;
use anyhow::{Context, Result};
use pyo3::prelude::PyAnyMethods;
use pyo3::types::PyDict;
use pyo3::{Py, PyAny, PyErr, Python};
use std::path::PathBuf;
use std::str::FromStr;

pub fn initialize_python(dir: &Dirs) -> Result<Py<PyAny>> {
    let appdata_dir = get_data_dir(dir)?;
    let appdata_dir = appdata_dir
        .to_str()
        .context("Failed to convert from pathbuf to str")?;
    let base_plugin_dir = PathBuf::from_str(&dir.default_plugins_dir)?.join("base");

    let pl_manager = Python::attach(|py| {
        let sys = py.import("sys")?;

        // aperio / aperio.gpu_util / aperio.logger / aperio.frame_structure をまとめて注入
        let modules = sys.getattr("modules")?;
        let modules = modules.cast::<PyDict>()?;
        wrapper::register_all(py, modules)?;

        // プラグインマネージャーのパスをsys.pathに追加
        let sys_path = sys.getattr("path")?;
        sys_path.call_method1("append", (&dir.plugin_manager_dir,))?;
        sys_path.call_method1("append", (&appdata_dir,))?;

        let sys_path: Vec<String> = sys.getattr("path")?.extract()?;
        println!("sys.path: {:?}", sys_path);

        // plmanagerのPluginManagerを初期化
        let pl_manager = py.import("aperio_plugin")?;
        let init_func = pl_manager.getattr("AperioManager")?;
        let pl_manager = init_func.call1((appdata_dir,))?;

        // pluginsにbaseがなければ追加する
        if !pl_manager
            .getattr("check_plugin_exists")?
            .call1(("AperioBasePlugin",))?
            .extract::<bool>()?
        {
            let add_plugin_func = pl_manager.getattr("add_plugin")?;
            add_plugin_func.call1((base_plugin_dir.to_str(),))?;
        }

        Ok::<Py<PyAny>, PyErr>(pl_manager.unbind())
    })?;

    Ok(pl_manager)
}

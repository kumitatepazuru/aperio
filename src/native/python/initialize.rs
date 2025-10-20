use std::path::PathBuf;
use std::str::FromStr;
use crate::dir_util::get_data_dir;
use anyhow::Result;
use pyo3::prelude::PyAnyMethods;
use pyo3::{Py, PyAny, PyErr, Python};
use crate::Dirs;

pub fn initialize_python(dir: &Dirs) -> Result<Py<PyAny>> {
    let appdata_dir = get_data_dir(dir)?;
    let resource_dir = PathBuf::from_str(&dir.resource_dir)?;
    let base_plugin_dir = resource_dir.join("default-plugins").join("base");

    // Pythonのプラグインシステムを初期化
    let pl_manager = Python::attach(|py| {
        // pythonのversionを取得
        let sys = py.import("sys")?;

        // sys.pathを表示
        let sys_path: Vec<String> = sys.getattr("path")?.extract()?;
        println!("sys.path: {:?}", sys_path);

        // plmanagerのPluginManagerを初期化
        let pl_manager = py.import("aperio_plugin")?;
        let init_func = pl_manager.getattr("PluginManager")?;
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

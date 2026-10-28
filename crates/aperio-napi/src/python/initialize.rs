use crate::managers::config_manager::AperioConfig;
use crate::python::modules::PyManagers;
use crate::python::{self, modules};
use crate::utils::{get_data_dir, get_local_data_dir};
use crate::Dirs;
use anyhow::{Context, Result};
use pyo3::prelude::PyAnyMethods;
use pyo3::types::PyDict;
use pyo3::{Py, PyAny, PyErr, Python};
use std::path::PathBuf;
use std::str::FromStr;

#[cfg(target_os = "linux")]
fn ensure_libpython_global(name: &str) -> anyhow::Result<()> {
    use std::ffi::CString;
    unsafe {
        let soname = CString::new(name)?; // 環境に合わせて調整
                                          // 既に読み込まれていれば GLOBAL に昇格
        let h = libc::dlopen(soname.as_ptr(), libc::RTLD_NOLOAD | libc::RTLD_GLOBAL);
        if h.is_null() {
            // 未ロードなら GLOBAL でロード
            let h2 = libc::dlopen(soname.as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL);
            assert!(!h2.is_null(), "failed to dlopen libpython with RTLD_GLOBAL");
        }

        Ok(())
    }
}

pub fn initialize_python(dirs: &Dirs, config: &AperioConfig) -> Result<()> {
    let default_version = &config.python.default_version;
    let local_data_dir = get_local_data_dir(dirs)?;
    let python_path = local_data_dir.join("python"); // pythonがある

    // pythonがインストールされているか確認
    // python環境変数の設定
    if !python_path.exists() {
        println!("Found no Python installation at {:?}", python_path);
        python::utils::install_python(dirs, default_version, true)?;
    }
    python::utils::add_python_path_env(dirs)?;

    let mut result = python::utils::check_python_installed(dirs)?;
    let mut try_count = 0;
    // TODO: try_countが3回を超えたら正しいエラーハンドリングをする
    while !result.installed && try_count < 3 {
        println!("Python is not installed. Installing...");
        python::utils::install_python(
            dirs,
            result.version.as_ref().unwrap_or(default_version),
            result.version.is_none(),
        )?;
        println!("Python installed");
        result = python::utils::check_python_installed(dirs)?;
        try_count += 1;
    }

    println!("Installed python version: {:?}", result.version);

    println!("syncing packages...");
    let sync_result = python::utils::sync_packages(dirs);
    println!("Package sync result: {:?}", sync_result);

    // Linuxの場合、libpythonをRTLD_GLOBALで読み込む
    #[cfg(target_os = "linux")]
    {
        // resourc_dir/app.asar.unpacked/dist/にあるlibpython*.so*を探す
        let entries = std::fs::read_dir(&dirs.dist_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if let Some(fname) = path.file_name().and_then(|s| s.to_str()) {
                if fname.starts_with("libpython") && fname.contains(".so") {
                    println!("Linux: Ensuring libpython global: {}", fname);
                    ensure_libpython_global(fname)?;
                }
            }
        }
    }

    Ok(())
}

pub fn initialize_pl_manager(dirs: &Dirs, py_managers: PyManagers) -> Result<Py<PyAny>> {
    let base_plugin_dir = PathBuf::from_str(&dirs.default_plugins_dir)?.join("base");
    let appdata_dir = get_data_dir(dirs)?;
    let appdata_dir = appdata_dir
        .to_str()
        .context("Failed to convert from pathbuf to str")?;

    let pl_manager = Python::attach(|py| {
        let sys = py.import("sys")?;

        // aperio / aperio.gpu_util / aperio.logger / aperio.item_structures をまとめて注入
        let modules = sys.getattr("modules")?;
        let modules = modules.cast::<PyDict>()?;
        modules::register_all(py, modules)?;

        // プラグインマネージャーのパスをsys.pathに追加
        let sys_path = sys.getattr("path")?;
        sys_path.call_method1("append", (&dirs.plugin_manager_dir,))?;
        sys_path.call_method1("append", (&appdata_dir,))?;

        let sys_path: Vec<String> = sys.getattr("path")?.extract()?;
        println!("sys.path: {:?}", sys_path);

        // plmanagerのPluginManagerを初期化
        let pl_manager = py.import("aperio_plugin")?;
        let init_func = pl_manager.getattr("AperioManager")?;
        let pl_manager = init_func.call1((appdata_dir, py_managers))?;

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

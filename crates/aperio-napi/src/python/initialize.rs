use crate::managers::config_manager::AperioConfig;
use crate::python::modules::PyManagers;
use crate::python::{self, modules};
use crate::utils::get_local_data_dir;
use crate::Dirs;
use anyhow::{Context, Result};
use pyo3::prelude::PyAnyMethods;
use pyo3::types::PyDict;
use pyo3::{Py, PyAny, PyErr, Python};
use std::collections::BTreeSet;
use std::path::Path;

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

// TODO: 関連funcをstructにまとめてdirsの管理とかをもうちょい楽にする
pub fn initialize_python(dirs: &Dirs, config: &AperioConfig, is_dev: bool) -> Result<()> {
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

    // ローカルのプラグインwheelを名前で解決できるよう、置き場所をindexとして登録する。
    // default_plugins_dirはインストール先によって変わるので毎回書き直す
    register_plugin_indexes(dirs)?;
    // 同梱のプラグイン(wheel)をaperio-envに追加する。既に同じものが入っていればuv側で差分なしになる
    install_default_plugins(dirs)?;

    println!("syncing packages...");
    let sync_result = python::utils::sync_packages(dirs, is_dev);
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

/// plugin_wheels(ユーザーが追加したwheel)と`default_plugins_dir`(同梱のwheel)をflat indexとして登録する。
fn register_plugin_indexes(dirs: &Dirs) -> Result<()> {
    let plugin_wheels_dir = python::utils::plugin_wheels_dir(dirs)?;
    // 存在しないディレクトリがindexにあるとuvが失敗するので作っておく
    std::fs::create_dir_all(&plugin_wheels_dir)?;
    python::utils::set_plugin_indexes(
        dirs,
        &[
            (python::utils::PLUGIN_WHEELS_INDEX, &plugin_wheels_dir),
            (python::utils::DEFAULT_PLUGINS_INDEX, Path::new(&dirs.default_plugins_dir)),
        ],
    )
}

/// default_plugins_dirにある同梱プラグインのwheelをすべてaperio-envに追加する。
fn install_default_plugins(dirs: &Dirs) -> Result<()> {
    // 同じパッケージの別バージョンのwheelが並んでいても1回だけaddする
    let mut packages = BTreeSet::new();
    for entry in std::fs::read_dir(&dirs.default_plugins_dir)
        .with_context(|| format!("Failed to read default plugins dir: {}", dirs.default_plugins_dir))?
    {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "whl") {
            packages.insert(
                python::utils::wheel_package_name(&path)
                    .with_context(|| format!("Invalid wheel file name: {:?}", path))?,
            );
        }
    }

    if packages.is_empty() {
        println!("No default plugin wheels found in {}", dirs.default_plugins_dir);
        return Ok(());
    }

    println!("Installing default plugins: {:?}", packages);
    python::utils::install_packages(
        dirs,
        packages.iter().map(String::as_str).collect(),
        Some(python::utils::DEFAULT_PLUGINS_INDEX),
    )
}

pub fn initialize_pl_manager(dirs: &Dirs, py_managers: PyManagers) -> Result<Py<PyAny>> {
    let pl_manager = Python::attach(|py| {
        let sys = py.import("sys")?;

        // aperio / aperio.gpu_util / aperio.logger / aperio.item_structures をまとめて注入
        let modules = sys.getattr("modules")?;
        let modules = modules.cast::<PyDict>()?;
        modules::register_all(py, modules)?;

        // プラグインマネージャー(aperio_plugin)のパスをsys.pathに追加する。
        // プラグイン自体はuvでaperio-envにインストールされ、entry pointから読み込まれる
        let sys_path = sys.getattr("path")?;
        sys_path.call_method1("append", (&dirs.plugin_manager_dir,))?;

        let sys_path: Vec<String> = sys.getattr("path")?.extract()?;
        println!("sys.path: {:?}", sys_path);

        // aperio_pluginのPluginManagerを初期化し、プラグインを読み込む
        let pl_manager = py
            .import("aperio_plugin")?
            .call_method1("initialize", (py_managers,))?;

        Ok::<Py<PyAny>, PyErr>(pl_manager.unbind())
    })?;

    Ok(pl_manager)
}

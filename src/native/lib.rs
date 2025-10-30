use crate::{app_config::read_config, dir_util::get_local_data_dir};
use napi_derive::napi;
use pyo3::{Py, PyAny};
use std::sync::Arc;
use tokio::sync::broadcast;
mod app_config;
mod dir_util;
mod python;

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

#[napi(object)]
pub struct Dirs {
    pub data_dir: String,
    pub local_data_dir: String,
    pub resource_dir: String,
    pub plugin_manager_dir: String,
    pub default_plugins_dir: String,
    pub dist_dir: String,
}

pub async fn _initialize(dirs: &Dirs) -> anyhow::Result<Py<PyAny>> {
    // configの初期化
    app_config::init_config(dirs)?;
    let config = read_config(dirs)?;
    let default_version = config.python.default_version;
    let local_data_dir = get_local_data_dir(dirs)?;
    let python_path = local_data_dir.join("python"); // pythonがある

    // pythonがインストールされているか確認
    // python環境変数の設定
    if !python_path.exists() {
        println!("Found no Python installation at {:?}", python_path);
        python::utils::install_python(
            dirs,
            &default_version,
            true,
        )
        .await?;
    }
    python::utils::add_python_path_env(dirs)?;


    let mut result = python::utils::check_python_installed(dirs).await?;
    let mut try_count = 0;
    // TODO: try_countが3回を超えたら正しいエラーハンドリングをする
    while !result.installed && try_count < 3 {
        println!("Python is not installed. Installing...");
        python::utils::install_python(
            dirs,
            result.version.as_ref().unwrap_or(&default_version),
            result.version.is_none(),
        )
        .await?;
        println!("Python installed");
        result = python::utils::check_python_installed(dirs).await?;
        try_count += 1;
    }

    println!("Installed python version: {:?}", result.version);

    println!("syncing packages...");
    let sync_result = python::utils::sync_packages(dirs).await;
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
    // python環境の初期化
    let pl_manager = python::initialize::initialize_python(dirs)?;
    let (tx, _) = broadcast::channel::<Arc<Vec<u8>>>(100);

    Ok(pl_manager)
}

#[napi(js_name = "PlManager")]
pub struct JsPlManager {
    plmanager: Option<Py<PyAny>>,
    dirs: Dirs,
}

// 一部IDEでanalyserが誤ってエラーを出すため注意
// 対処方法は(RustRoverの場合)現状ない模様
#[napi]
impl JsPlManager {
    #[napi(constructor)]
    pub fn new(dirs: Dirs) -> Self {
        Self {
            plmanager: None,
            dirs,
        }
    }

    #[napi]
    pub async unsafe fn initialize(&mut self) -> napi::Result<()> {
        let result = _initialize(&self.dirs).await;
        let pl_manager = result.map_err(|e| {
            eprintln!("Failed to initialize Python environment: {:?}", e);

            napi::Error::from_reason(format!("Failed to initialize Python environment: {:?}", e))
        })?;

        // 内部情報の更新
        self.plmanager = Some(pl_manager);

        Ok(())
    }
}

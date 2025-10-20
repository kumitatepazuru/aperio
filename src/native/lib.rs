use crate::app_config::read_config;
use std::sync::Arc;
use napi_derive::napi;
use pyo3::{Py, PyAny};
use tokio::sync::broadcast;
mod app_config;
mod dir_util;
mod python;
mod error;

#[napi(object)]
pub struct Dirs {
    pub data_dir: String,
    pub local_data_dir: String,
    pub resource_dir: String,
}

#[napi(js_name = "PlManager")]
pub struct JsPlManager {
    plmanager: Option<Py<PyAny>>,
    dirs: Dirs,
}

#[napi]
impl JsPlManager {
    #[napi(factory)]
    pub fn new(dirs: Dirs) -> Self {
        Self {
            plmanager: None,
            dirs,
        }
    }

    #[napi]
    pub async unsafe fn initialize(&mut self) -> napi::Result<()> {
        // configの初期化
        app_config::init_config(&self.dirs)?;
        let config = read_config(&self.dirs)?;
        let default_version = config.python.default_version;

        // pythonがインストールされているか確認
        // python環境変数の設定
        python::utils::add_python_path_env(&self.dirs)?;
        let mut result = python::utils::check_python_installed(&self.dirs).await?;
        let mut try_count = 0;
        while !result.installed && try_count < 3 {
            println!("Python is not installed. Installing...");
            let python_installed = python::utils::install_python(
                &self.dirs,
                result.version.as_ref().unwrap_or(&default_version),
                result.version.is_none(),
            )
                .await;
            println!("Python installed: {:?}", python_installed);
            result = python::utils::check_python_installed(&self.dirs).await?;
            try_count += 1;
        }

        println!("Installed python version: {:?}", result.version);

        println!("syncing packages...");
        let sync_result = python::utils::sync_packages(&self.dirs).await;
        println!("Package sync result: {:?}", sync_result);

        // python環境の初期化
        let pl_manager = python::initialize::initialize_python(&self.dirs)?;
        let (tx, _) = broadcast::channel::<Arc<Vec<u8>>>(100);

        // 内部情報の更新
        self.plmanager = Some(pl_manager);

        Ok(())
    }
}
use crate::{
    app_config::read_config,
    node_shared_texture::{NodeOffscreenSharedTextureInfo, NodeSharedTextureFormat},
    structs::{Dirs, FrameLayerStructure},
    util::get_local_data_dir,
};
#[cfg(target_os = "linux")]
use gpu_util::texture_to_native::linux::SharedTextureHandle;
#[cfg(target_os = "windows")]
use gpu_util::texture_to_native::windows::SharedTextureHandle;

use gpu_util::{PySharedTextureHandle, SharedTextureFormat};
use log::debug;
use napi::bindgen_prelude::Uint8ArraySlice;
use napi_derive::napi;
use pyo3::{types::PyAnyMethods, IntoPyObject, Py, PyAny, PyResult, Python};
mod app_config;
mod node_shared_texture;
mod python;
mod structs;
mod util;

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

pub fn _initialize(dirs: &Dirs) -> anyhow::Result<Py<PyAny>> {
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
        python::utils::install_python(dirs, &default_version, true)?;
    }
    python::utils::add_python_path_env(dirs)?;

    let mut result = python::utils::check_python_installed(dirs)?;
    let mut try_count = 0;
    // TODO: try_countが3回を超えたら正しいエラーハンドリングをする
    while !result.installed && try_count < 3 {
        println!("Python is not installed. Installing...");
        python::utils::install_python(
            dirs,
            result.version.as_ref().unwrap_or(&default_version),
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
    // python環境の初期化
    let pl_manager = python::initialize::initialize_python(dirs)?;

    Ok(pl_manager)
}

#[napi]
pub struct PlManager {
    plmanager: Option<Py<PyAny>>,
    dirs: Dirs,
}

// 一部IDEでanalyserが誤ってエラーを出すため注意
// 対処方法は(RustRoverの場合)現状ない模様
#[napi]
impl PlManager {
    #[napi(constructor)]
    pub fn new(dirs: Dirs) -> Self {
        let _ = env_logger::try_init(); // すでに初期化されている場合は無視
        match env_logger::try_init() {
            Ok(()) => {}
            Err(e) => {
                // すでに初期化されている場合は無視するが、デバッグ用にログを出力
                debug!("env_logger initialization skipped: {}", e);
            }
        }
        
        Self {
            plmanager: None,
            dirs,
        }
    }

    #[napi]
    pub fn initialize(&mut self) -> napi::Result<()> {
        let result = _initialize(&self.dirs);
        let pl_manager = result.map_err(|e| {
            eprintln!("Failed to initialize Python environment: {:?}", e);

            napi::Error::from_reason(format!("Failed to initialize Python environment: {:?}", e))
        })?;

        // 内部情報の更新
        self.plmanager = Some(pl_manager);

        Ok(())
    }

    #[napi]
    pub fn get_frame_buf(
        &self,
        #[napi(ts_arg_type = "Uint8Array")] mut buffer: Uint8ArraySlice,
        count: i32,
        frame_struct: Vec<FrameLayerStructure>,
    ) -> napi::Result<()> {
        let pl_manager = self
            .plmanager
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("PluginManager is not initialized"))?;

        Python::attach(|py| -> PyResult<()> {
            let pl_manager = pl_manager.bind(py);
            let frame_struct = frame_struct.into_pyobject(py)?;

            let buffer_ptr = unsafe {
                let buffer_slice = buffer.as_mut();
                buffer_slice.as_mut_ptr() as usize
            };

            let func = pl_manager.getattr("make_frame_buf")?;
            func.call1((count, frame_struct, 1920, 1080, buffer_ptr))?;

            Ok(())
        })
        .map_err(|e| napi::Error::from_reason(format!("Failed to get frame: {:?}", e)))?;

        Ok(())
    }

    #[napi]
    pub fn get_frame_texture(
        &self,
        count: i32,
        frame_struct: Vec<FrameLayerStructure>,
        base_texture: NodeOffscreenSharedTextureInfo,
        format: NodeSharedTextureFormat,
    ) -> napi::Result<()> {
        let pl_manager = self
            .plmanager
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("PluginManager is not initialized"))?;
        let content_size = base_texture.coded_size;
        // rgbaf16またはbgraではない場合はエラー
        if base_texture.pixel_format != "rgbaf16" && base_texture.pixel_format != "bgra" {
            return Err(napi::Error::from_reason(
                "Base texture pixel format must be 'rgbaf16' or 'bgra'".to_string(),
            ));
        }

        let output = Python::attach(|py| -> PyResult<()> {
            let pl_manager = pl_manager.bind(py);
            let frame_struct = frame_struct.into_pyobject(py)?;
            let base_texture: SharedTextureHandle = base_texture.handle.into();
            let base_texture = PySharedTextureHandle::new(base_texture);
            let format: SharedTextureFormat = format.into();

            let func = pl_manager.getattr("make_frame_shared_texture")?;
            func.call1((
                count,
                frame_struct,
                content_size.width,
                content_size.height,
                base_texture,
                format,
            ))?;

            Ok(())
        })
        .map_err(|e| napi::Error::from_reason(format!("Failed to get frame: {:?}", e)))?;

        Ok(output)
    }
}

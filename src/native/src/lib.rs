use std::collections::HashMap;

use crate::{
    app_config::{AperioConfig, AperioConfigManager},
    structs::{frame_structure::*, node_shared_texture::NodeOffscreenSharedTextureInfo},
    utils::{get_local_data_dir, json_value::JsonValue},
};
#[cfg(target_os = "linux")]
use gpu_util::texture_to_native::linux::SharedTextureHandle;
#[cfg(target_os = "windows")]
use gpu_util::texture_to_native::windows::SharedTextureHandle;

// Python公開用の型は wrapper クレートから使用する
use crate::python::modules::{
    gpu_util::{PySharedTextureHandle, WrappedSharedTextureFormat},
    text_rendering::FontsList,
};
use aperio_derive::impl_plugin_event_methods;
use log::debug;
use napi::bindgen_prelude::Uint8ArraySlice;
use napi_derive::napi;
use pyo3::prelude::*;

mod app_config;
mod python;
mod store;
mod structs;
mod utils;
mod audio;

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

fn _initialize(dirs: &Dirs, config: &AperioConfig) -> anyhow::Result<Py<PyAny>> {
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
    // python環境の初期化
    let pl_manager = python::initialize::initialize_python(dirs)?;

    Ok(pl_manager)
}

#[napi]
pub struct AperioManager {
    plmanager: Py<PyAny>,
}

// 一部IDEでanalyserが誤ってエラーを出すため挿入
//noinspection RsCompileErrorMacro
#[napi]
impl AperioManager {
    #[napi(constructor)]
    pub fn new(dirs: Dirs, config_manager: &AperioConfigManager) -> napi::Result<Self> {
        match env_logger::try_init() {
            Ok(()) => {}
            Err(e) => {
                // すでに初期化されている場合は無視するが、デバッグ用にログを出力
                debug!("env_logger initialization skipped: {}", e);
            }
        }
        let config = config_manager.get_config();
        let plmanager = _initialize(&dirs, &config).map_err(|e| {
            napi::Error::from_reason(format!("Failed to initialize Python environment: {:?}", e))
        })?;

        Ok(Self { plmanager })
    }

    #[napi]
    pub fn get_plugin_names(&self) -> napi::Result<PluginNameInfo> {
        let pl_manager = &self.plmanager;

        let result = Python::attach(|py| -> PyResult<PluginNameInfo> {
            let pl_manager = pl_manager.bind(py);
            let names = pl_manager.call_method0("get_plugin_names")?;
            Ok(names.extract()?)
        })
        .map_err(|e| napi::Error::from_reason(format!("Failed to get plugin names: {:?}", e)))?;

        Ok(result)
    }

    #[napi]
    pub fn get_frame_buf(
        &self,
        #[napi(ts_arg_type = "Uint8Array")] mut buffer: Uint8ArraySlice,
        count: i32,
        width: i32,
        height: i32,
        fps: f64,
        frame_struct: Vec<ItemStructure>,
    ) -> napi::Result<HashMap<String, ItemResult>> {
        let pl_manager = &self.plmanager;

        let output = Python::attach(|py| -> PyResult<HashMap<String, ItemResult>> {
            let pl_manager = pl_manager.bind(py);

            let buffer_ptr = unsafe {
                let buffer_slice = buffer.as_mut();
                buffer_slice.as_mut_ptr() as usize
            };

            let func = pl_manager.getattr("make_frame_buf")?;
            let results: HashMap<String, ItemResult> = func
                .call1((count, frame_struct, width, height, fps, buffer_ptr))?
                .extract()?;

            Ok(results)
        })
        .map_err(|e| napi::Error::from_reason(format!("Failed to get frame: {:?}", e)))?;

        Ok(output)
    }

    #[napi]
    pub fn get_frame_texture(
        &self,
        count: i32,
        tex_format: WrappedSharedTextureFormat,
        fps: f64,
        frame_struct: Vec<ItemStructure>,
        base_texture: NodeOffscreenSharedTextureInfo,
    ) -> napi::Result<HashMap<String, ItemResult>> {
        let pl_manager = &self.plmanager;

        let content_size = base_texture.coded_size;
        // formatとbase_texture.pixel_formatが一致してなければエラー
        if (tex_format == WrappedSharedTextureFormat::Rgba16Float
            && base_texture.pixel_format != "rgbaf16")
            || (tex_format == WrappedSharedTextureFormat::Bgra8Unorm
                && base_texture.pixel_format != "bgra")
        {
            return Err(napi::Error::from_reason(format!(
                "Pixel format mismatch: expected {:?}, got {}",
                tex_format, base_texture.pixel_format
            )));
        }

        let output = Python::attach(|py| -> PyResult<HashMap<String, ItemResult>> {
            let pl_manager = pl_manager.bind(py);

            let base_texture: SharedTextureHandle = base_texture.handle.into();
            let base_texture = PySharedTextureHandle::new(base_texture);

            let func = pl_manager.getattr("make_frame_shared_texture")?;
            let results: HashMap<String, ItemResult> = func
                .call1((
                    count,
                    frame_struct,
                    content_size.width,
                    content_size.height,
                    fps,
                    base_texture,
                    tex_format,
                ))?
                .extract()?;

            Ok(results)
        })
        .map_err(|e| napi::Error::from_reason(format!("Failed to get frame: {:?}", e)))?;

        Ok(output)
    }

    /// システムにインストールされているフォントの一覧を返す。
    /// 戻り値はフォントファミリー名 → ウェイト値（100/200/…/900）の配列。
    #[napi]
    pub fn get_fonts_list(&self) -> napi::Result<FontsList> {
        let pl_manager = &self.plmanager;

        let result = Python::attach(|py| -> PyResult<FontsList> {
            let pl_manager = pl_manager.bind(py);
            Ok(pl_manager.call_method0("get_fonts_list")?.extract()?)
        })
        .map_err(|e| napi::Error::from_reason(format!("Failed to get fonts list: {:?}", e)))?;

        Ok(result)
    }
}

// イベント系メソッドは #[napi] impl の外でマクロ展開する
impl_plugin_event_methods! {
    AperioManager {
        request_new(GeneratorEvent::New): JsonValue -> GeneratorInformation,
        request_structure(GeneratorEvent::RequestStructure): JsonValue -> GeneratorInformation,
    }
}

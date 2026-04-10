use std::collections::HashMap;

use crate::{
    app_config::{AperioConfig, AperioConfigManager},
    node_shared_texture::NodeOffscreenSharedTextureInfo,
    structs::Dirs,
    util::get_local_data_dir,
};
#[cfg(target_os = "linux")]
use gpu_util::texture_to_native::linux::SharedTextureHandle;
#[cfg(target_os = "windows")]
use gpu_util::texture_to_native::windows::SharedTextureHandle;

// Python公開用の型は wrapper クレートから使用する
use log::debug;
use napi::bindgen_prelude::Uint8ArraySlice;
use napi_derive::napi;
use pyo3::{types::PyAnyMethods, Py, PyAny, PyResult, Python};
use wrapper::{
    frame_structure::*,
    gpu_util::{PySharedTextureHandle, WrappedSharedTextureFormat},
    utils::json_to_pyobject,
};
use pyo3::IntoPyObject;

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

// 一部IDEでanalyserが誤ってエラーを出すため注意
// 対処方法は(RustRoverの場合)現状ない模様
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
    pub fn request_new_object_generator(
        &self,
        plugin_name: String,
        args: serde_json::Value,
    ) -> napi::Result<NewObjectGeneratorReturn> {
        let pl_manager = &self.plmanager;

        let result = Python::attach(|py| -> PyResult<NewObjectGeneratorReturn> {
            let pl_manager = pl_manager.bind(py);
            let gen_info = pl_manager.call_method1(
                "request_new_object_generator",
                (plugin_name, json_to_pyobject(py, &args)?),
            )?;
            Ok(gen_info.extract()?)
        })
        .map_err(|e| {
            napi::Error::from_reason(format!("Failed to request new object generator: {:?}", e))
        })?;

        Ok(result)
    }

    #[napi]
    pub fn request_new_effect_generator(
        &self,
        plugin_name: String,
    ) -> napi::Result<NewEffectGeneratorReturn> {
        let pl_manager = &self.plmanager;

        let result = Python::attach(|py| -> PyResult<NewEffectGeneratorReturn> {
            let pl_manager = pl_manager.bind(py);
            let gen_info =
                pl_manager.call_method1("request_new_effect_generator", (plugin_name,))?;
            Ok(gen_info.extract()?)
        })
        .map_err(|e| {
            napi::Error::from_reason(format!("Failed to request new effect generator: {:?}", e))
        })?;

        Ok(result)
    }

    #[napi]
    pub fn request_parameter_struct(
        &self,
        plugin_name: String,
        params: serde_json::Value,
    ) -> napi::Result<Vec<RequestStructureParameter>> {
        let pl_manager = &self.plmanager;

        let result = Python::attach(|py| -> PyResult<Vec<RequestStructureParameter>> {
            let pl_manager = pl_manager.bind(py);
            let struct_info = pl_manager.call_method1(
                "request_parameter_struct",
                (plugin_name, json_to_pyobject(py, &params)?),
            )?;
            Ok(struct_info.extract()?)
        })
        .map_err(|e| {
            napi::Error::from_reason(format!("Failed to get parameter struct: {:?}", e))
        })?;

        Ok(result)
    }

    #[napi]
    pub fn get_frame_buf(
        &self,
        #[napi(ts_arg_type = "Uint8Array")] mut buffer: Uint8ArraySlice,
        count: i32,
        width: i32,
        height: i32,
        frame_struct: Vec<ItemStructure>,
    ) -> napi::Result<HashMap<String, ItemResult>> {
        let pl_manager = &self.plmanager;

        let output = Python::attach(|py| -> PyResult<HashMap<String, ItemResult>> {
            let pl_manager = pl_manager.bind(py);
            let frame_struct = frame_struct.into_pyobject(py)?;

            let buffer_ptr = unsafe {
                let buffer_slice = buffer.as_mut();
                buffer_slice.as_mut_ptr() as usize
            };

            let func = pl_manager.getattr("make_frame_buf")?;
            let results: HashMap<String, ItemResult> = func
                .call1((count, frame_struct, width, height, buffer_ptr))?
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
            let frame_struct = frame_struct.into_pyobject(py)?;

            let base_texture: SharedTextureHandle = base_texture.handle.into();
            let base_texture = PySharedTextureHandle::new(base_texture);

            let func = pl_manager.getattr("make_frame_shared_texture")?;
            let results: HashMap<String, ItemResult> = func
                .call1((
                    count,
                    frame_struct,
                    content_size.width,
                    content_size.height,
                    base_texture,
                    tex_format,
                ))?
                .extract()?;

            Ok(results)
        })
        .map_err(|e| napi::Error::from_reason(format!("Failed to get frame: {:?}", e)))?;

        Ok(output)
    }
}

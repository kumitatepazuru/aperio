use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use crate::{
    managers::{
        audio_manager::AudioManager,
        config_manager::ConfigManager,
        store_manager::StoreManager,
        wrappers::{
            audio_manager::{NapiAudioManager, PyAudioManager},
            config_manager::{NapiConfigManager, PyConfigManager},
            store_manager::{NapiStoreManager, PyStoreManager},
        },
    },
    python::modules::PyManagers,
    structs::{frame_structure::*, node_shared_texture::NodeOffscreenSharedTextureInfo},
    utils::json_value::JsonValue,
};
#[cfg(target_os = "linux")]
use gpu_util::texture_to_native::linux::SharedTextureHandle;
#[cfg(target_os = "windows")]
use gpu_util::texture_to_native::windows::SharedTextureHandle;

use crate::python::modules::{
    gpu_util::{PySharedTextureHandle, WrappedSharedTextureFormat},
    text_rendering::FontsList,
};
use aperio_derive::impl_plugin_event_methods;
use log::debug;
use napi::bindgen_prelude::{JavaScriptClassExt, Reference, Uint8ArraySlice};
use napi::Env;
use napi_derive::napi;
use pyo3::prelude::*;

mod managers;
mod python;
mod structs;
mod utils;

#[napi(object)]
#[derive(Clone)]
pub struct Dirs {
    pub data_dir: String,
    pub local_data_dir: String,
    pub resource_dir: String,
    pub plugin_manager_dir: String,
    pub default_plugins_dir: String,
    pub dist_dir: String,
}

#[napi]
pub struct AperioManager {
    plmanager: Py<PyAny>,
    config_manager: Reference<NapiConfigManager>,
    audio_manager: Reference<NapiAudioManager>,
    store_manager: Reference<NapiStoreManager>,
}

// 一部IDEでanalyserが誤ってエラーを出すため挿入
//noinspection RsCompileErrorMacro
#[napi]
impl AperioManager {
    #[napi(constructor)]
    pub fn new(env: Env, dirs: Dirs) -> napi::Result<Self> {
        match env_logger::try_init() {
            Ok(()) => {}
            Err(e) => {
                debug!("env_logger initialization skipped: {}", e);
            }
        }

        // 内部 struct を生成
        let config_inner = ConfigManager::new(&dirs)?;
        let config = config_inner.get_config();

        let default_audio = crate::managers::store_manager::default_audio_state();
        let audio_inner = AudioManager::new(
            default_audio.channels,
            default_audio.sample_rate,
            default_audio.bit_depth,
        )
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;

        // Arc でラップして共有所有権を確立
        let config_arc = Arc::new(Mutex::new(config_inner));
        let audio_arc = Arc::new(Mutex::new(audio_inner));
        let store_arc = Arc::new(RwLock::new(StoreManager::new_with_audio(Arc::clone(
            &audio_arc,
        ))));

        // napi wrapper を生成して Reference に変換
        let config_ref = NapiConfigManager::new(Arc::clone(&config_arc)).into_reference(env)?;
        let audio_ref = NapiAudioManager::new(Arc::clone(&audio_arc)).into_reference(env)?;
        let store_ref = NapiStoreManager::new(Arc::clone(&store_arc)).into_reference(env)?;

        // Python 環境初期化
        python::initialize::initialize_python(&dirs, &config).map_err(|e| {
            napi::Error::from_reason(format!("Failed to initialize Python environment: {:?}", e))
        })?;

        // pyo3 wrapper を生成して PyManagers に格納
        let py_managers = Python::attach(|py| -> PyResult<PyManagers> {
            Ok(PyManagers {
                config_manager: Py::new(py, PyConfigManager::new(Arc::clone(&config_arc)))?,
                audio_manager: Py::new(py, PyAudioManager::new(Arc::clone(&audio_arc)))?,
                store_manager: Py::new(py, PyStoreManager::new(Arc::clone(&store_arc)))?,
            })
        })
        .map_err(|e| {
            napi::Error::from_reason(format!("Failed to create Python managers: {:?}", e))
        })?;

        // プラグインマネージャー初期化
        let plmanager =
            python::initialize::initialize_pl_manager(&dirs, py_managers).map_err(|e| {
                napi::Error::from_reason(format!("Failed to initialize plugin manager: {:?}", e))
            })?;

        Ok(Self {
            plmanager,
            config_manager: config_ref,
            audio_manager: audio_ref,
            store_manager: store_ref,
        })
    }

    #[napi(getter)]
    pub fn config_manager(&self, env: Env) -> napi::Result<Reference<NapiConfigManager>> {
        self.config_manager.clone(env)
    }

    #[napi(getter)]
    pub fn audio_manager(&self, env: Env) -> napi::Result<Reference<NapiAudioManager>> {
        self.audio_manager.clone(env)
    }

    #[napi(getter)]
    pub fn store_manager(&self, env: Env) -> napi::Result<Reference<NapiStoreManager>> {
        self.store_manager.clone(env)
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

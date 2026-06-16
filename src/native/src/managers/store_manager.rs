use anyhow::{anyhow, Result};
use aperio_derive::{generate_partial, PyDataclass};
use napi_derive::napi;
use pyo3::prelude::*;
use std::sync::{Arc, Mutex, RwLock};

use crate::managers::audio_manager::AudioManager;
use crate::structs::frame_structure::ItemStructure;
use crate::utils::NullOr;

#[napi(object)]
#[pyclass(from_py_object, get_all, eq)]
#[derive(Clone, PartialEq, PyDataclass, Debug)]
pub struct ViewerState {
    #[napi(ts_type = "'playing' | 'paused'")]
    pub state: String,
    pub change_time: f64,
    pub begin_frame: i32,
}

#[napi(object)]
#[pyclass(from_py_object, get_all, eq)]
#[derive(Clone, PartialEq, PyDataclass, Debug)]
pub struct FrameState {
    pub width: i32,
    pub height: i32,
    pub fps: f64,
}

#[napi(object)]
#[pyclass(from_py_object, get_all, eq)]
#[derive(Clone, PartialEq, PyDataclass, Debug)]
pub struct ColorPickerState {
    #[napi(ts_type = "'HSV' | 'LCH' | 'okLCH' | 'LAB' | 'okLAB'")]
    pub color_space: String,
    #[napi(ts_type = "'0-1' | '0-255'")]
    pub display_mode: String,
    /// ColorValue = [r, g, b, a] (0.0〜1.0)
    #[napi(ts_type = "[number, number, number, number][]")]
    pub history: Vec<Vec<f64>>,
}

#[napi(object)]
#[pyclass(from_py_object, get_all, eq)]
#[derive(Clone, PartialEq, PyDataclass, Debug)]
pub struct AudioState {
    pub channels: u16,
    pub sample_rate: u32,
    pub bit_depth: u32,
}

generate_partial! {
    /// renderer 間で同期する直列化可能なアプリケーション状態。
    #[napi(object)]
    #[pyclass(from_py_object, get_all, eq)]
    #[derive(Clone, PartialEq, PyDataclass)]
    pub struct SyncableState {
        pub viewer_state: ViewerState,
        pub timeline_items: Vec<ItemStructure>,
        pub frame_state: FrameState,
        pub audio_state: AudioState,
        pub selected_item_ids: Vec<String>,
        #[napi(ts_type = "string | null")]
        pub main_selected_item_id: NullOr<String>,
        pub color_picker: ColorPickerState,
    }

    /// SyncableState の部分更新用。各フィールドが None の場合はその項目を更新しない。
    /// main_selected_item_id を null にクリアしたい場合は Some(NullOr::Null) を渡す。
    /// ここに書いていないフィールドは SyncableState から Option<T> で自動補完される。
    #[napi(object)]
    #[derive(Clone)]
    #[pyclass(from_py_object)]
    pub struct SyncableStatePartial {
        #[napi(ts_type = "string | null | undefined")]
        pub main_selected_item_id: Option<NullOr<String>>,
    }
}

pub(crate) fn default_audio_state() -> AudioState {
    AudioState {
        channels: 2,
        sample_rate: 44100,
        bit_depth: 16,
    }
}

fn default_state() -> SyncableState {
    SyncableState {
        viewer_state: ViewerState {
            state: "paused".to_string(),
            change_time: 0.0,
            begin_frame: 0,
        },
        frame_state: FrameState {
            width: 1920,
            height: 1080,
            fps: 60.0,
        },
        audio_state: default_audio_state(),
        timeline_items: vec![],
        selected_item_ids: vec![],
        main_selected_item_id: NullOr::Null,
        color_picker: ColorPickerState {
            color_space: "HSV".to_string(),
            display_mode: "0-255".to_string(),
            history: vec![],
        },
    }
}

pub struct StoreManager {
    state: RwLock<SyncableState>,
    audio: Arc<Mutex<AudioManager>>,
}

impl StoreManager {
    pub fn new_with_audio(audio: Arc<Mutex<AudioManager>>) -> Self {
        Self {
            state: RwLock::new(default_state()),
            audio,
        }
    }

    pub fn get_state(&self) -> Result<SyncableState> {
        self.state
            .read()
            .map(|s| s.clone())
            .map_err(|e| anyhow!(e.to_string()))
    }

    pub fn set_partial(&mut self, mut partial: SyncableStatePartial) -> Result<()> {
        if let Some(ref new_audio) = partial.audio_state {
            let current_audio = self
                .state
                .read()
                .map(|s| s.audio_state.clone())
                .map_err(|e| anyhow!(e.to_string()))?;
            let mut audio = self
                .audio
                .lock()
                .map_err(|_| anyhow!("audio mutex poisoned"))?;
            if new_audio.channels != current_audio.channels {
                audio.set_channels(new_audio.channels)?;
            }
            if new_audio.sample_rate != current_audio.sample_rate {
                audio.set_sample_rate(new_audio.sample_rate)?;
            }
            if new_audio.bit_depth != current_audio.bit_depth {
                audio.set_bit_depth(new_audio.bit_depth);
            }
        }
        let mut s = self.state.write().map_err(|e| anyhow!(e.to_string()))?;
        partial.apply_to(&mut s);
        Ok(())
    }
}

use anyhow::{anyhow, Result};
use aperio_derive::{ApplyPartial, PyDataclass};
use napi_derive::napi;
use pyo3::prelude::*;
use std::sync::{OnceLock, RwLock};

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

/// renderer 間で同期する直列化可能なアプリケーション状態。
#[napi(object)]
#[pyclass(from_py_object, get_all, eq)]
#[derive(Clone, PartialEq, PyDataclass)]
pub struct SyncableState {
    pub viewer_state: ViewerState,
    pub timeline_items: Vec<ItemStructure>,
    pub frame_state: FrameState,
    pub selected_item_ids: Vec<String>,
    #[napi(ts_type = "string | null")]
    pub main_selected_item_id: NullOr<String>,
    pub color_picker: ColorPickerState,
}

/// SyncableState の部分更新用。各フィールドが None の場合はその項目を更新しない。
/// main_selected_item_id を null にクリアしたい場合は Some(NullOr::Null) を渡す。
#[napi(object)]
#[derive(ApplyPartial, Clone)]
#[pyclass(from_py_object)]
#[apply_partial(target = SyncableState)]
pub struct SyncableStatePartial {
    pub viewer_state: Option<ViewerState>,
    pub timeline_items: Option<Vec<ItemStructure>>,
    pub frame_state: Option<FrameState>,
    pub selected_item_ids: Option<Vec<String>>,
    #[napi(ts_type = "string | null | undefined")]
    pub main_selected_item_id: Option<NullOr<String>>,
    pub color_picker: Option<ColorPickerState>,
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

static STORE_STATE: OnceLock<RwLock<SyncableState>> = OnceLock::new();

fn state() -> &'static RwLock<SyncableState> {
    STORE_STATE.get_or_init(|| RwLock::new(default_state()))
}

#[pyfunction]
#[napi]
pub fn store_get_state() -> Result<SyncableState> {
    state()
        .read()
        .map(|s| s.clone())
        .map_err(|e| anyhow!(e.to_string()))
}

#[pyfunction]
#[napi]
pub fn store_set_partial(mut partial: SyncableStatePartial) -> Result<()> {
    let mut s = state().write().map_err(|e| anyhow!(e.to_string()))?;

    partial.apply_to(&mut s);
    Ok(())
}

#[pymodule]
pub mod store {
    #[pymodule_export]
    use super::store_get_state;
    #[pymodule_export]
    use super::store_set_partial;
    #[pymodule_export]
    use super::ColorPickerState;
    #[pymodule_export]
    use super::FrameState;
    #[pymodule_export]
    use super::SyncableState;
    #[pymodule_export]
    use super::SyncableStatePartial;
    #[pymodule_export]
    use super::ViewerState;
}

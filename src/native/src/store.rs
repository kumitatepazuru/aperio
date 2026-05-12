use aperio_derive::ApplyPartial;
use napi_derive::napi;
use std::sync::{OnceLock, RwLock};
use wrapper::frame_structure::ItemStructure;

#[napi(object)]
#[derive(Clone)]
pub struct ViewerState {
    #[napi(ts_type = "'playing' | 'paused'")]
    pub state: String,
    pub change_time: f64,
    pub begin_frame: i32,
}

#[napi(object)]
#[derive(Clone)]
pub struct FrameState {
    pub width: i32,
    pub height: i32,
    pub fps: f64,
}

#[napi(object)]
#[derive(Clone)]
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
/// main_selected_item_id は None のとき JS 側では null になる（ts_type 注記参照）。
#[napi(object)]
#[derive(Clone)]
pub struct SyncableState {
    pub viewer_state: ViewerState,
    pub timeline_items: Vec<ItemStructure>,
    pub frame_state: FrameState,
    pub selected_item_ids: Vec<String>,
    #[napi(ts_type = "string | null")]
    pub main_selected_item_id: Option<String>,
    pub color_picker: ColorPickerState,
}

/// SyncableState の部分更新用。各フィールドが None の場合はその項目を更新しない。
/// main_selected_item_id を null にしたい場合は selected_item_ids を空配列にすること
/// （apply_to が自動的に None にクリアする）。
#[napi(object)]
#[derive(ApplyPartial)]
#[apply_partial(target = SyncableState, skip = [selected_item_ids, main_selected_item_id])]
pub struct SyncableStatePartial {
    pub viewer_state: Option<ViewerState>,
    pub timeline_items: Option<Vec<ItemStructure>>,
    pub frame_state: Option<FrameState>,
    pub selected_item_ids: Option<Vec<String>>,
    pub main_selected_item_id: Option<String>,
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
        main_selected_item_id: None,
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

#[napi]
pub fn store_get_state() -> napi::Result<SyncableState> {
    state()
        .read()
        .map(|s| s.clone())
        .map_err(|e| napi::Error::from_reason(e.to_string()))
}

#[napi]
pub fn store_set_partial(mut partial: SyncableStatePartial) -> napi::Result<()> {
    let mut s = state()
        .write()
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;

    partial.apply_to(&mut s);

    if let Some(ids) = partial.selected_item_ids {
        if ids.is_empty() {
            s.main_selected_item_id = None;
        }
        s.selected_item_ids = ids;
    }
    if let Some(main_id) = partial.main_selected_item_id {
        s.main_selected_item_id = Some(main_id);
    }

    Ok(())
}

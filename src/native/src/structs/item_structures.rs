use std::collections::HashMap;

use aperio_derive::PyDataclass;
use napi_derive::napi;
use pyo3::{prelude::*, types::PyDict};

use crate::utils::json_value::JsonValue;

fn opt_add_f64(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    if a.is_none() && b.is_none() {
        None
    } else {
        Some(a.unwrap_or(0.0) + b.unwrap_or(0.0))
    }
}

fn opt_add_tuple3_f64(a: Option<(f64, f64, f64)>, b: Option<(f64, f64, f64)>) -> Option<(f64, f64, f64)> {
    match (a, b) {
        (None, None) => None,
        (Some((x1, y1, z1)), None) => Some((x1, y1, z1)),
        (None, Some((x2, y2, z2))) => Some((x2, y2, z2)),
        (Some((x1, y1, z1)), Some((x2, y2, z2))) => Some((x1 + x2, y1 + y2, z1 + z2)),
    }
}

fn opt_mul_tuple2_f64(a: Option<(f64, f64)>, b: Option<(f64, f64)>) -> Option<(f64, f64)> {
    match (a, b) {
        (None, None) => None,
        (Some((x1, y1)), None) => Some((x1, y1)),
        (None, Some((x2, y2))) => Some((x2, y2)),
        (Some((x1, y1)), Some((x2, y2))) => Some((x1 * x2, y1 * y2)),
    }
}

fn opt_mul_f64(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (None, None) => None,
        (Some(v1), None) => Some(v1),
        (None, Some(v2)) => Some(v2),
        (Some(v1), Some(v2)) => Some(v1 * v2),
    }
}

#[napi(object)]
#[pyclass(from_py_object, get_all, eq)]
#[derive(Clone, PartialEq, Debug, PyDataclass)]
pub struct FileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

#[napi(object)]
#[pyclass(from_py_object, get_all, eq)]
#[derive(Clone, PartialEq, Debug)]
pub struct ItemResult {
    pub width: i32,
    pub height: i32,
    pub pos: Option<(f64, f64, f64)>,
    // 回転・拡縮の基点オフセット。実機は Q12(1/4096px)で持つので、半画素の
    // マージン(`領域拡張` の `(左-右)/2`)を落とさないよう整数ではなく実数。
    pub center_x: Option<f64>,
    pub center_y: Option<f64>,
    pub scale: Option<(f64, f64)>,
    pub rotate: Option<(f64, f64, f64)>,
    pub alpha: Option<f64>,
    // エフェクトが「自分の背面/前面に追加のアイテムを流し込みたい」場合に使う。
    // 合成ループは additional_item.item を実アイテムと全く同じコードパスで処理する。
    pub additional_item: Option<AdditionalItem>,
}

#[pymethods]
impl ItemResult {
    #[new]
    #[pyo3(signature = (width, height, pos=None, center_x=None, center_y=None, scale=None, rotate=None, alpha=None, additional_item=None))]
    fn new(
        width: i32,
        height: i32,
        pos: Option<(f64, f64, f64)>,
        center_x: Option<f64>,
        center_y: Option<f64>,
        scale: Option<(f64, f64)>,
        rotate: Option<(f64, f64, f64)>,
        alpha: Option<f64>,
        additional_item: Option<AdditionalItem>,
    ) -> Self {
        Self {
            width,
            height,
            pos,
            center_x,
            center_y,
            scale,
            rotate,
            alpha,
            additional_item,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "ItemResult(width={:?}, height={:?}, pos={:?}, center_x={:?}, center_y={:?}, scale={:?}, rotate={:?}, alpha={:?}, additional_item={:?})",
            self.width, self.height, self.pos, self.center_x, self.center_y, self.scale, self.rotate, self.alpha, self.additional_item
        )
    }

    fn combine(&mut self, item_result: ItemResult) {
        // サイズは「そのエフェクトが出力したテクスチャの大きさ」なので後勝ち。
        // 何もしないエフェクトも入力と同じ width/height を返すため無害。
        self.width = item_result.width;
        self.height = item_result.height;
        self.pos = opt_add_tuple3_f64(self.pos, item_result.pos);
        self.center_x = opt_add_f64(self.center_x, item_result.center_x);
        self.center_y = opt_add_f64(self.center_y, item_result.center_y);
        self.scale = opt_mul_tuple2_f64(self.scale, item_result.scale);
        self.rotate = opt_add_tuple3_f64(self.rotate, item_result.rotate);
        self.alpha = opt_mul_f64(self.alpha, item_result.alpha);
    }
}

#[napi]
#[pyclass(from_py_object)]
#[derive(Clone, Debug, PartialEq)]
pub enum RequestStructureParameter {
    #[pyo3(constructor = (id, title, default_value, suffix=None, min=None, max=None))]
    Float {
        id: String,
        title: String,
        default_value: f64,
        suffix: Option<String>,
        min: Option<f64>,
        max: Option<f64>,
    },
    #[pyo3(constructor = (id, title, default_value, suffix=None, min=None, max=None))]
    Int {
        id: String,
        title: String,
        default_value: i64,
        suffix: Option<String>,
        min: Option<i64>,
        max: Option<i64>,
    },
    Bool {
        id: String,
        title: String,
        default_value: bool,
    },
    #[pyo3(constructor = (id, title, default_value, suffix=None, min=None, max=None))]
    Vec2Int {
        id: String,
        title: String,
        default_value: (i32, i32),
        suffix: Option<String>,
        min: Option<i32>,
        max: Option<i32>,
    },
    #[pyo3(constructor = (id, title, default_value, suffix=None, min=None, max=None))]
    Vec2Float {
        id: String,
        title: String,
        default_value: (f64, f64),
        suffix: Option<String>,
        min: Option<f64>,
        max: Option<f64>,
    },
    #[pyo3(constructor = (id, title, default_value, suffix=None, min=None, max=None))]
    Vec3Int {
        id: String,
        title: String,
        default_value: (i32, i32, i32),
        suffix: Option<String>,
        min: Option<i32>,
        max: Option<i32>,
    },
    #[pyo3(constructor = (id, title, default_value, suffix=None, min=None, max=None))]
    Vec3Float {
        id: String,
        title: String,
        default_value: (f64, f64, f64),
        suffix: Option<String>,
        min: Option<f64>,
        max: Option<f64>,
    },
    #[pyo3(constructor = (id, title, default_value, suffix=None, min=None, max=None))]
    Vec4Int {
        id: String,
        title: String,
        default_value: (i32, i32, i32, i32),
        suffix: Option<String>,
        min: Option<i32>,
        max: Option<i32>,
    },
    #[pyo3(constructor = (id, title, default_value, suffix=None, min=None, max=None))]
    Vec4Float {
        id: String,
        title: String,
        default_value: (f64, f64, f64, f64),
        suffix: Option<String>,
        min: Option<f64>,
        max: Option<f64>,
    },
    String {
        id: String,
        title: String,
        default_value: String,
    },
    Color {
        id: String,
        title: String,
        default_value: (f64, f64, f64, f64), // RGBA形式で0.0〜1.0の範囲
        use_alpha: bool,
    },
    List {
        id: String,
        title: String,
        values: HashMap<String, String>, // key: valueのペア
        default_value: String,
    },
    Font {
        id: String,
        title: String,
        // デフォルト値は固定: family=None, weight=400
    },
    Textarea {
        id: String,
        title: String,
        default_value: String,
    },
    File {
        id: String,
        title: String,
        multi_selections: bool,
        open_type: String, // "file" | "directory"
        filters: Vec<FileFilter>,
    },
}

#[napi(object)]
#[pyclass(from_py_object, extends = PyDict)]
#[derive(Clone, IntoPyObject, PartialEq, Debug)]
pub struct GenerateStructure {
    pub id: String,   // UUIDが期待される
    pub name: String, // オブジェクトやエフェクトの固有名でIDとは違い種類が同じであれば同じになる
    pub display_name: String,
    pub parameters: HashMap<String, JsonValue>, // パラメータの具体的な型はエフェクトによって異なるため、単にdict(JsonValue)型とする
    // Some の場合、このエントリは実体を持たず「link_id が指す別エントリの計算結果を
    // フレーム内で使い回す」ことだけを意味する(name/display_name/parameters は無視される)。
    pub link_id: Option<String>,
}

#[pymethods]
impl GenerateStructure {
    /// Python側から additionalItem 用に GenerateStructure を新規構築するためのコンストラクタ。
    #[new]
    #[pyo3(signature = (id, name, display_name, parameters, link_id=None))]
    fn new(
        py: Python<'_>,
        id: String,
        name: String,
        display_name: String,
        parameters: Py<PyDict>,
        link_id: Option<String>,
    ) -> PyResult<Self> {
        Ok(Self {
            id,
            name,
            display_name,
            parameters: parameters.bind(py).extract()?,
            link_id,
        })
    }
}

#[napi]
#[pyclass(from_py_object)]
#[derive(Clone, Debug, PartialEq)]
pub enum ItemStructure {
    #[pyo3(constructor = (id, layer, start, end, x, y, scale, rotation, alpha, object, effects, min=None, max=None))]
    Video {
        id: String,
        layer: i32,
        start: i32, // アイテムの開始フレーム
        end: i32,   // アイテムの終了フレーム
        x: i32,
        y: i32,
        scale: f64,
        rotation: f64,
        alpha: f64,
        object: GenerateStructure,
        effects: Vec<GenerateStructure>,
        min: Option<i32>, // アイテムの有効な最小フレーム（省略可能）
        max: Option<i32>, // アイテムの有効な最大フレーム（省略可能）
    },
    #[pyo3(constructor = (id, layer, start, end, volume, pan, object, effects, min=None, max=None))]
    Audio {
        id: String,
        layer: i32,
        start: i32, // アイテムの開始フレーム
        end: i32,   // アイテムの終了フレーム
        volume: f64,
        pan: f64,
        object: GenerateStructure,
        effects: Vec<GenerateStructure>,
        min: Option<i32>,
        max: Option<i32>,
    },
}

#[napi(object)]
#[pyclass(from_py_object, get_all, eq)]
#[derive(Clone, PartialEq, Debug, PyDataclass)]
pub struct AdditionalItem {
    pub item: ItemStructure,
    /// true = 挿入元アイテムより背面(下)に挿入 / false = 前面(上)に挿入
    pub behind: bool,
}

#[napi(object)]
#[pyclass(from_py_object, get_all, eq)]
#[derive(Clone, PartialEq, Debug, PyDataclass)]
pub struct GeneratorInformation {
    pub display_name: String,
    pub duration_frames: Option<i32>,
    pub max_frame: Option<i32>,
    pub min_frame: Option<i32>,
    pub structure: Vec<RequestStructureParameter>,
}

#[napi]
#[pyclass(eq, from_py_object)]
#[derive(PartialEq, Clone, Debug)]
pub enum GeneratorEvent {
    New,
    RequestStructure,
}

#[napi(object)]
#[pyclass(from_py_object, get_all, eq)]
#[derive(Clone, PartialEq, Debug, PyDataclass)]
pub struct PluginNameInfo {
    pub base_plugin: HashMap<String, String>,
    pub video_object_plugins: HashMap<String, String>,
    pub video_effect_plugins: HashMap<String, String>,
    pub audio_object_plugins: HashMap<String, String>,
    pub audio_effect_plugins: HashMap<String, String>,
}

// ---- モジュール登録 -------------------------------------------------------

#[pymodule(module = "aperio.item_structures")]
pub mod item_structures {
    #[pymodule_export]
    use super::AdditionalItem;
    #[pymodule_export]
    use super::FileFilter;
    #[pymodule_export]
    use super::GenerateStructure;
    #[pymodule_export]
    use super::GeneratorEvent;
    #[pymodule_export]
    use super::GeneratorInformation;
    #[pymodule_export]
    use super::ItemResult;
    #[pymodule_export]
    use super::ItemStructure;
    #[pymodule_export]
    use super::PluginNameInfo;
    #[pymodule_export]
    use super::RequestStructureParameter;
}

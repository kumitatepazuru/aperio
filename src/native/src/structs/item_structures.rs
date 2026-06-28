use std::collections::HashMap;

use aperio_derive::PyDataclass;
use napi_derive::napi;
use pyo3::{prelude::*, types::PyDict};

use crate::utils::json_value::JsonValue;

#[napi(object)]
#[pyclass(from_py_object, get_all, eq)]
#[derive(Clone, PartialEq, Debug, PyDataclass)]
pub struct FileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

#[napi(object)]
#[pyclass(from_py_object, get_all, eq)]
#[derive(Clone, PartialEq, Debug, PyDataclass)]
pub struct ItemResult {
    pub width: i32,
    pub height: i32,
}

#[napi]
#[pyclass(from_py_object)]
#[derive(Clone, Debug, PartialEq)]
pub enum RequestStructureParameter {
    Float {
        id: String,
        title: String,
        default_value: f64,
        suffix: Option<String>,
    },
    Int {
        id: String,
        title: String,
        default_value: i64,
        suffix: Option<String>,
    },
    Bool {
        id: String,
        title: String,
        default_value: bool,
    },
    Vec2Int {
        id: String,
        title: String,
        default_value: (i32, i32),
        suffix: Option<String>,
    },
    Vec2Float {
        id: String,
        title: String,
        default_value: (f64, f64),
        suffix: Option<String>,
    },
    Vec3Int {
        id: String,
        title: String,
        default_value: (i32, i32, i32),
        suffix: Option<String>,
    },
    Vec3Float {
        id: String,
        title: String,
        default_value: (f64, f64, f64),
        suffix: Option<String>,
    },
    Vec4Int {
        id: String,
        title: String,
        default_value: (i32, i32, i32, i32),
        suffix: Option<String>,
    },
    Vec4Float {
        id: String,
        title: String,
        default_value: (f64, f64, f64, f64),
        suffix: Option<String>,
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
}

#[napi]
#[pyclass(from_py_object)]
#[derive(Clone, Debug, PartialEq)]
pub enum ItemStructure {
    Video {
        id: String,
        layer: i32,
        start: i32,                      // アイテムの開始フレーム
        end: i32,                        // アイテムの終了フレーム
        min: Option<i32>,                // アイテムの有効な最小フレーム（省略可能）
        max: Option<i32>,                // アイテムの有効な最大フレーム（省略可能）
        x: i32,
        y: i32,
        scale: f64,
        rotation: f64,
        alpha: f64,
        object: GenerateStructure,
        effects: Vec<GenerateStructure>,
    },
    Audio {
        id: String,
        layer: i32,
        start: i32,                      // アイテムの開始フレーム
        end: i32,                        // アイテムの終了フレーム
        min: Option<i32>,
        max: Option<i32>,
        volume: f64,
        pan: f64,
        object: GenerateStructure,
        effects: Vec<GenerateStructure>,
    },
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

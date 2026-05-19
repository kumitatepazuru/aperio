use std::collections::HashMap;

use aperio_derive::PyDataclass;
use napi_derive::napi;
use pyo3::{prelude::*, types::PyDict};

use crate::utils::json_value::JsonValue;

#[napi(object)]
#[pyo3::pyclass(from_py_object, get_all, eq)]
#[derive(Clone, PartialEq, Debug, PyDataclass)]
pub struct FileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

#[napi(object)]
#[pyo3::pyclass(from_py_object, get_all, eq)]
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
#[derive(Clone, IntoPyObject)]
pub struct GenerateStructure {
    pub id: String,   // UUIDが期待される
    pub name: String, // オブジェクトやエフェクトの固有名でIDとは違い種類が同じであれば同じになる
    pub display_name: String,
    pub parameters: HashMap<String, JsonValue>, // パラメータの具体的な型はエフェクトによって異なるため、単にdict(JsonValue)型とする
}

#[napi(object)]
#[pyclass(from_py_object, extends = PyDict)]
#[derive(Clone, IntoPyObject)]
pub struct ItemStructure {
    pub id: String,                      // アイテムのUUID
    pub layer: i32,                      // アイテムのレイヤー（整数、0が最背面）
    pub from: i32,                       // アイテムの開始フレーム
    pub to: i32,                         // アイテムの終了フレーム
    pub min: Option<i32>,                // アイテムの有効な最小フレーム（省略可能）
    pub max: Option<i32>,                // アイテムの有効な最大フレーム（省略可能）
    pub x: i32,                          // アイテムのX座標
    pub y: i32,                          // アイテムのY座標
    pub scale: f64,                      // アイテムのスケール
    pub rotation: f64,                   // アイテムの回転角度（度数法）
    pub alpha: f64,                      // アイテムの不透明度（0.0〜1.0）
    pub object: GenerateStructure,       // ベースとなるオブジェクトプラグインの情報
    pub effects: Vec<GenerateStructure>, // 適用されるエフェクトプラグインの情報のリスト
}

#[napi(object)]
#[pyo3::pyclass(from_py_object, get_all, eq)]
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
#[pyo3::pyclass(from_py_object, get_all, eq)]
#[derive(Clone, PartialEq, Debug, PyDataclass)]
pub struct PluginNameInfo {
    pub base_plugin: HashMap<String, String>,
    pub object_plugins: HashMap<String, String>,
    pub effect_plugins: HashMap<String, String>,
}

// ---- モジュール登録 -------------------------------------------------------

#[pymodule]
pub mod frame_structure {
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

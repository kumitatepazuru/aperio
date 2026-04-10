use std::collections::HashMap;

use aperio_derive::pydataclass;
use napi_derive::napi;
use pyo3::{
    prelude::*,
    types::{PyDict, PyList, PyModuleMethods},
};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_complex_enum};

use crate::utils::json_to_pyobject;

#[napi(object)]
#[pydataclass(stub, module = "aperio.frame_structure")]
pub struct ItemResult {
    pub width: i32,
    pub height: i32,
}

#[napi]
#[gen_stub_pyclass_complex_enum]
#[pyclass(module = "aperio.frame_structure")]
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
}

#[napi(object)]
#[gen_stub_pyclass]
#[pyclass(module = "aperio.frame_structure", extends = PyDict)]
#[derive(Clone)]
pub struct GenerateStructure {
    pub id: String,   // UUIDが期待される
    pub name: String, // オブジェクトやエフェクトの固有名でIDとは違い種類が同じであれば同じになる
    pub display_name: String,
    pub parameters: HashMap<String, serde_json::Value>, // パラメータの具体的な型はエフェクトによって異なるため、単にdict(serde_json::Value)型とする
}

#[napi(object)]
#[gen_stub_pyclass]
#[pyclass(module = "aperio.frame_structure", extends = PyDict)]
pub struct ItemStructure {
    pub id: String,                      // アイテムのUUID
    pub x: i32,                          // アイテムのX座標
    pub y: i32,                          // アイテムのY座標
    pub scale: f64,                      // アイテムのスケール
    pub rotation: f64,                   // アイテムの回転角度（度数法）
    pub alpha: f64,                      // アイテムの不透明度（0.0〜1.0）
    pub object: GenerateStructure,       // ベースとなるオブジェクトプラグインの情報
    pub effects: Vec<GenerateStructure>, // 適用されるエフェクトプラグインの情報のリスト
}

#[napi(object)]
#[pydataclass(stub, module = "aperio.frame_structure")]
pub struct NewObjectGeneratorReturn {
    pub display_name: String,
    pub duration_frames: i32,
    pub structure: Vec<RequestStructureParameter>,
}

#[napi(object)]
#[pydataclass(stub, module = "aperio.frame_structure")]
pub struct NewEffectGeneratorReturn {
    pub display_name: String,
    pub structure: Vec<RequestStructureParameter>,
}

#[napi(object)]
#[pydataclass(stub, module = "aperio.frame_structure")]
pub struct PluginNameInfo {
    pub base_plugin: HashMap<String, String>,
    pub object_plugins: HashMap<String, String>,
    pub effect_plugins: HashMap<String, String>,
}

// ---- IntoPyObject 実装 ---------------------------------------------------

impl<'py> IntoPyObject<'py> for GenerateStructure {
    type Target = PyDict;
    type Output = Bound<'py, PyDict>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        let dict = PyDict::new(py);
        dict.set_item("id", self.id)?;
        dict.set_item("name", self.name)?;
        dict.set_item("display_name", self.display_name)?;
        let params = PyDict::new(py);
        for (k, v) in self.parameters {
            params.set_item(k, json_to_pyobject(py, &v)?)?;
        }
        dict.set_item("parameters", params)?;
        Ok(dict)
    }
}

impl<'py> IntoPyObject<'py> for ItemStructure {
    type Target = PyDict;
    type Output = Bound<'py, PyDict>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        let dict = PyDict::new(py);
        dict.set_item("id", self.id)?;
        dict.set_item("x", self.x)?;
        dict.set_item("y", self.y)?;
        dict.set_item("scale", self.scale)?;
        dict.set_item("rotation", self.rotation)?;
        dict.set_item("alpha", self.alpha)?;
        dict.set_item("object", self.object.into_pyobject(py)?)?;
        let effects = self
            .effects
            .into_iter()
            .map(|e| e.into_pyobject(py))
            .collect::<Result<Vec<_>, _>>()?;
        dict.set_item("effects", PyList::new(py, effects)?)?;
        Ok(dict)
    }
}

// ---- モジュール登録 -------------------------------------------------------

pub fn register(m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<ItemResult>()?;
    m.add_class::<RequestStructureParameter>()?;
    m.add_class::<GenerateStructure>()?;
    m.add_class::<ItemStructure>()?;
    m.add_class::<NewObjectGeneratorReturn>()?;
    m.add_class::<NewEffectGeneratorReturn>()?;
    m.add_class::<PluginNameInfo>()?;
    Ok(())
}

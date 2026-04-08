use std::collections::HashMap;

use anyhow::Result;
use napi_derive::napi;
use pyo3::{
    exceptions::PyValueError,
    types::{PyAnyMethods, PyDict},
    Borrowed, Bound, FromPyObject, IntoPyObject, IntoPyObjectExt, PyAny, PyErr, PyResult, Python,
};

use crate::util::json_to_pyobject;

#[napi(object)]
pub struct Dirs {
    pub data_dir: String,
    pub local_data_dir: String,
    pub resource_dir: String,
    pub plugin_manager_dir: String,
    pub default_plugins_dir: String,
    pub dist_dir: String,
}

// TODO: python側と共通化する
// from: /src-python/src/aperio_plugin/types/frame_structure.py
#[napi]
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

impl<'a, 'py> FromPyObject<'a, 'py> for RequestStructureParameter {
    type Error = PyErr;

    fn extract(ob: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        macro_rules! variant {
            ($v:ident { $($f:ident),* }) => {
                Ok(RequestStructureParameter::$v {
                    $($f: ob.getattr(stringify!($f))?.extract()?,)*
                })
            };
        }
        let class_name: String = ob.getattr("__class__")?.getattr("__name__")?.extract()?;
        match class_name.as_str() {
            "FloatParam" => variant!(Float {
                id,
                title,
                default_value,
                suffix
            }),
            "IntParam" => variant!(Int {
                id,
                title,
                default_value,
                suffix
            }),
            "BoolParam" => variant!(Bool {
                id,
                title,
                default_value
            }),
            "Vec2IntParam" => variant!(Vec2Int {
                id,
                title,
                default_value,
                suffix
            }),
            "Vec2FloatParam" => variant!(Vec2Float {
                id,
                title,
                default_value,
                suffix
            }),
            "Vec3IntParam" => variant!(Vec3Int {
                id,
                title,
                default_value,
                suffix
            }),
            "Vec3FloatParam" => variant!(Vec3Float {
                id,
                title,
                default_value,
                suffix
            }),
            "Vec4IntParam" => variant!(Vec4Int {
                id,
                title,
                default_value,
                suffix
            }),
            "Vec4FloatParam" => variant!(Vec4Float {
                id,
                title,
                default_value,
                suffix
            }),
            "StringParam" => variant!(String {
                id,
                title,
                default_value
            }),
            "ColorParam" => variant!(Color {
                id,
                title,
                default_value,
                use_alpha
            }),
            "ListParam" => variant!(List {
                id,
                title,
                values,
                default_value
            }),
            _ => Err(PyValueError::new_err(format!(
                "Unknown RequestStructureParameter type: {}",
                class_name
            ))),
        }
    }
}

#[napi(object)]
pub struct GenerateStructure {
    pub id: String,   // UUIDが期待される
    pub name: String, // オブジェクトやエフェクトの固有名でIDとは違い種類が同じであれば同じになる
    pub display_name: String,
    pub parameters: HashMap<String, serde_json::Value>,
}

#[napi(object)]
#[derive(IntoPyObject)]
pub struct LayerStructure {
    pub id: String, // UUIDが期待される
    pub x: i32,
    pub y: i32,
    pub scale: f64,
    pub rotation: f64,
    pub alpha: f64,
    pub obj: GenerateStructure,
    pub effects: Vec<GenerateStructure>,
}

impl<'a, 'py> IntoPyObject<'py> for &'a GenerateStructure {
    type Target = PyAny;
    type Output = Bound<'py, Self::Target>;
    type Error = pyo3::PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        let dict = PyDict::new(py);
        dict.set_item("id", &self.id)?;
        dict.set_item("name", &self.name)?;
        dict.set_item("display_name", &self.display_name)?;

        let params_dict = PyDict::new(py);
        for (k, v) in &self.parameters {
            params_dict.set_item(k, json_to_pyobject(py, v)?)?;
        }
        dict.set_item("parameters", params_dict)?;
        Ok(dict.into_bound_py_any(py)?)
    }
}

impl<'py> IntoPyObject<'py> for GenerateStructure {
    type Target = PyAny;
    type Output = Bound<'py, Self::Target>;
    type Error = pyo3::PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        (&self).into_pyobject(py)
    }
}

#[napi(object)]
#[derive(FromPyObject)]
pub struct NewObjectGeneratorReturn {
    pub display_name: String,
    pub duration_frames: i32,
    pub structure: Vec<RequestStructureParameter>,
}

#[napi(object)]
#[derive(FromPyObject)]
pub struct NewEffectGeneratorReturn {
    pub display_name: String,
    pub structure: Vec<RequestStructureParameter>,
}

#[napi(object)]
#[derive(FromPyObject)]
pub struct PluginNameInfo {
    pub base_plugin: HashMap<String, String>,
    pub object_plugins: HashMap<String, String>,
    pub effect_plugins: HashMap<String, String>,
}

#[napi(object)]
#[derive(FromPyObject)]
pub struct LayerResult {
    pub width: i32,
    pub height: i32,
}

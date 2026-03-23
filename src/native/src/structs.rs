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
        default_x: i32,
        default_y: i32,
        suffix: Option<String>,
    },
    Vec2Float {
        id: String,
        title: String,
        default_x: f64,
        default_y: f64,
        suffix: Option<String>,
    },
    Vec3Int {
        id: String,
        title: String,
        default_x: i32,
        default_y: i32,
        default_z: i32,
        suffix: Option<String>,
    },
    Vec3Float {
        id: String,
        title: String,
        default_x: f64,
        default_y: f64,
        default_z: f64,
        suffix: Option<String>,
    },
    Vec4Int {
        id: String,
        title: String,
        default_x: i32,
        default_y: i32,
        default_z: i32,
        default_w: i32,
        suffix: Option<String>,
    },
    Vec4Float {
        id: String,
        title: String,
        default_x: f64,
        default_y: f64,
        default_z: f64,
        default_w: f64,
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
        default_r: u8,
        default_g: u8,
        default_b: u8,
        default_a: u8,
        use_alpha: bool,
    },
    List {
        id: String,
        title: String,
        values: HashMap<String, String>, // key: valueのペア
        default_key: String,
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
                default_x,
                default_y,
                suffix
            }),
            "Vec2FloatParam" => variant!(Vec2Float {
                id,
                title,
                default_x,
                default_y,
                suffix
            }),
            "Vec3IntParam" => variant!(Vec3Int {
                id,
                title,
                default_x,
                default_y,
                default_z,
                suffix
            }),
            "Vec3FloatParam" => variant!(Vec3Float {
                id,
                title,
                default_x,
                default_y,
                default_z,
                suffix
            }),
            "Vec4IntParam" => variant!(Vec4Int {
                id,
                title,
                default_x,
                default_y,
                default_z,
                default_w,
                suffix
            }),
            "Vec4FloatParam" => variant!(Vec4Float {
                id,
                title,
                default_x,
                default_y,
                default_z,
                default_w,
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
                default_r,
                default_g,
                default_b,
                default_a,
                use_alpha
            }),
            "ListParam" => variant!(List {
                id,
                title,
                values,
                default_key
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
    pub name: String,
    pub parameters: HashMap<String, serde_json::Value>,
}

#[napi(object)]
#[derive(IntoPyObject)]
pub struct LayerStructure {
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
        dict.set_item("name", &self.name)?;

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

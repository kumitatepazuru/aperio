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
        value: f64,
        suffix: Option<String>,
    },
    Int {
        id: String,
        title: String,
        value: i64,
        suffix: Option<String>,
    },
    Bool {
        id: String,
        title: String,
        value: bool,
    },
    Vec2Int {
        id: String,
        title: String,
        x: i32,
        y: i32,
        suffix: Option<String>,
    },
    Vec2Float {
        id: String,
        title: String,
        x: f64,
        y: f64,
        suffix: Option<String>,
    },
    Vec3Int {
        id: String,
        title: String,
        x: i32,
        y: i32,
        z: i32,
        suffix: Option<String>,
    },
    Vec3Float {
        id: String,
        title: String,
        x: f64,
        y: f64,
        z: f64,
        suffix: Option<String>,
    },
    Vec4Int {
        id: String,
        title: String,
        x: i32,
        y: i32,
        z: i32,
        w: i32,
        suffix: Option<String>,
    },
    Vec4Float {
        id: String,
        title: String,
        x: f64,
        y: f64,
        z: f64,
        w: f64,
        suffix: Option<String>,
    },
    String {
        id: String,
        title: String,
        value: String,
    },
    Color {
        id: String,
        title: String,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
        use_alpha: bool,
    },
    List {
        id: String,
        title: String,
        values: Vec<String>,
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
                value,
                suffix
            }),
            "IntParam" => variant!(Int {
                id,
                title,
                value,
                suffix
            }),
            "BoolParam" => variant!(Bool { id, title, value }),
            "Vec2IntParam" => variant!(Vec2Int {
                id,
                title,
                x,
                y,
                suffix
            }),
            "Vec2FloatParam" => variant!(Vec2Float {
                id,
                title,
                x,
                y,
                suffix
            }),
            "Vec3IntParam" => variant!(Vec3Int {
                id,
                title,
                x,
                y,
                z,
                suffix
            }),
            "Vec3FloatParam" => variant!(Vec3Float {
                id,
                title,
                x,
                y,
                z,
                suffix
            }),
            "Vec4IntParam" => variant!(Vec4Int {
                id,
                title,
                x,
                y,
                z,
                w,
                suffix
            }),
            "Vec4FloatParam" => variant!(Vec4Float {
                id,
                title,
                x,
                y,
                z,
                w,
                suffix
            }),
            "StringParam" => variant!(String { id, title, value }),
            "ColorParam" => variant!(Color {
                id,
                title,
                r,
                g,
                b,
                a,
                use_alpha
            }),
            "ListParam" => variant!(List { id, title, values }),
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
    pub parameters: serde_json::Value,
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
        dict.set_item("parameters", json_to_pyobject(py, &self.parameters)?)?;
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

use napi_derive::napi;
use pyo3::{
    types::{PyAnyMethods, PyDict},
    Bound, IntoPyObject, IntoPyObjectExt, PyAny, Python,
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

#[napi(object)]
pub struct GenerateStructure {
    pub name: String,
    pub parameters: serde_json::Value,
}

#[napi(object)]
#[derive(IntoPyObject)]
pub struct FrameLayerStructure {
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

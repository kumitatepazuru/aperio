use pyo3::{
    types::{PyDict, PyDictMethods, PyList, PyListMethods},
    Bound, IntoPyObjectExt, Py, PyAny, PyErr, PyResult, Python,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JsonValue(pub serde_json::Value);

impl From<serde_json::Value> for JsonValue {
    fn from(v: serde_json::Value) -> Self {
        JsonValue(v)
    }
}

impl From<JsonValue> for serde_json::Value {
    fn from(v: JsonValue) -> Self {
        v.0
    }
}

impl napi::bindgen_prelude::FromNapiValue for JsonValue {
    unsafe fn from_napi_value(
        env: napi::sys::napi_env,
        napi_val: napi::sys::napi_value,
    ) -> napi::Result<Self> {
        Ok(JsonValue(serde_json::Value::from_napi_value(env, napi_val)?))
    }
}

impl napi::bindgen_prelude::ToNapiValue for JsonValue {
    unsafe fn to_napi_value(
        env: napi::sys::napi_env,
        val: Self,
    ) -> napi::Result<napi::sys::napi_value> {
        serde_json::Value::to_napi_value(env, val.0)
    }
}

impl napi::bindgen_prelude::TypeName for JsonValue {
    fn type_name() -> &'static str {
        "Object"
    }
    fn value_type() -> napi::ValueType {
        napi::ValueType::Object
    }
}

impl napi::bindgen_prelude::ValidateNapiValue for JsonValue {}

impl<'py> pyo3::IntoPyObject<'py> for JsonValue {
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        value_to_pyobject(py, self.0)
    }
}

fn value_to_pyobject<'py>(
    py: Python<'py>,
    v: serde_json::Value,
) -> PyResult<Bound<'py, PyAny>> {
    Ok(match v {
        serde_json::Value::Null => py.None().into_bound_py_any(py)?,
        serde_json::Value::Bool(b) => b.into_bound_py_any(py)?,
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_bound_py_any(py)?
            } else if let Some(u) = n.as_u64() {
                u.into_bound_py_any(py)?
            } else if let Some(f) = n.as_f64() {
                f.into_bound_py_any(py)?
            } else {
                py.None().into_bound_py_any(py)?
            }
        }
        serde_json::Value::String(s) => s.into_bound_py_any(py)?,
        serde_json::Value::Array(arr) => {
            let list = PyList::new(py, Vec::<Py<PyAny>>::new())?;
            for item in arr {
                list.append(value_to_pyobject(py, item)?)?;
            }
            list.into_bound_py_any(py)?
        }
        serde_json::Value::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                dict.set_item(k, value_to_pyobject(py, v)?)?;
            }
            dict.into_bound_py_any(py)?
        }
    })
}

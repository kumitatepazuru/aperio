// ---- serde_json → Python 変換 --------------------------------------------

use pyo3::{
    types::{PyDict, PyDictMethods, PyList, PyListMethods},
    Bound, IntoPyObjectExt, Py, PyAny, PyResult, Python,
};
use serde_json::Value;
pub fn json_to_pyobject<'py>(py: Python<'py>, v: &Value) -> PyResult<Bound<'py, PyAny>> {
    Ok(match v {
        Value::Null => py.None().into_bound_py_any(py)?,
        Value::Bool(b) => b.into_bound_py_any(py)?,
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_bound_py_any(py)?
            } else if let Some(u) = n.as_u64() {
                // u64 は Python の任意精度 int にそのまま入る
                u.into_bound_py_any(py)?
            } else if let Some(f) = n.as_f64() {
                f.into_bound_py_any(py)?
            } else {
                // serde_json::Number は通常ここに来ないはず
                // TODO: Warningを出す
                py.None().into_bound_py_any(py)?
            }
        }
        Value::String(s) => s.into_bound_py_any(py)?,
        Value::Array(arr) => {
            let list = PyList::new(py, vec![] as Vec<Py<PyAny>>)?;
            for item in arr {
                list.append(json_to_pyobject(py, item)?)?;
            }
            list.into_bound_py_any(py)?
        }
        Value::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                dict.set_item(k, json_to_pyobject(py, v)?)?;
            }
            dict.into_bound_py_any(py)?
        }
    })
}

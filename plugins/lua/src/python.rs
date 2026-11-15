//! Python バインディング(`python` feature)。[`crate::runtime::LuaRuntime`] の薄いラッパー。

use std::sync::Arc;

use pyo3::exceptions::{PyKeyError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyFloat, PyInt, PyString, PyTuple, PyType};

use crate::runtime::{self, Callback, CallbackError, LuaRuntime, Value, ValueType};

/// LuaJIT のランタイム。変数・関数・プロシージャを登録してスクリプトを実行する。
#[pyclass(name = "LuaRuntime", module = "lua_runtime")]
pub struct PyLuaRuntime {
    inner: LuaRuntime,
}

fn to_pyerr(err: runtime::Error) -> PyErr {
    let message = err.to_string();
    match err {
        runtime::Error::Callback(e) => match e.downcast::<PyErr>() {
            Ok(e) => *e,
            Err(e) => PyRuntimeError::new_err(e.to_string()),
        },
        runtime::Error::Type(_) => PyTypeError::new_err(message),
        runtime::Error::UnknownVariable(_) => PyKeyError::new_err(message),
        runtime::Error::InvalidName(_) => PyValueError::new_err(message),
        runtime::Error::Lua(_) => PyRuntimeError::new_err(message),
    }
}

fn is_type(obj: &Bound<'_, PyAny>, ty: &Bound<'_, PyType>) -> bool {
    obj.as_ptr() == ty.as_ptr()
}

/// `float` / `int` / `str` / `bool` の型オブジェクトを [`ValueType`] に変換する。
fn parse_value_type(obj: &Bound<'_, PyAny>) -> PyResult<ValueType> {
    let py = obj.py();
    if is_type(obj, &py.get_type::<PyFloat>()) {
        Ok(ValueType::Float)
    } else if is_type(obj, &py.get_type::<PyBool>()) {
        Ok(ValueType::Bool)
    } else if is_type(obj, &py.get_type::<PyInt>()) {
        Ok(ValueType::Int)
    } else if is_type(obj, &py.get_type::<PyString>()) {
        Ok(ValueType::Str)
    } else {
        Err(PyTypeError::new_err(format!(
            "type must be one of float, int, str, bool, got {}",
            obj.repr()?
        )))
    }
}

/// 戻り値型。`None` / `type(None)` は戻り値なし。
fn parse_return_type(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Option<ValueType>> {
    match obj {
        None => Ok(None),
        Some(obj) if obj.is_none() || is_type(obj, &obj.py().None().bind(obj.py()).get_type()) => {
            Ok(None)
        }
        Some(obj) => parse_value_type(obj).map(Some),
    }
}

fn parse_arg_types(arg_types: &Bound<'_, PyTuple>) -> PyResult<Vec<ValueType>> {
    arg_types.iter().map(|t| parse_value_type(&t)).collect()
}

/// Python の値を宣言型の [`Value`] に変換する。`float` は `int` も受け付ける(`bool` は不可)。
fn py_to_value(obj: &Bound<'_, PyAny>, ty: ValueType) -> PyResult<Value> {
    let is_bool = obj.is_instance_of::<PyBool>();
    let is_int = obj.is_instance_of::<PyInt>() && !is_bool;
    let value = match ty {
        ValueType::Float if obj.is_instance_of::<PyFloat>() || is_int => {
            Some(Value::Float(obj.extract::<f64>()?))
        }
        ValueType::Int if is_int => Some(Value::Int(obj.extract::<i64>()?)),
        ValueType::Str if obj.is_instance_of::<PyString>() => Some(Value::Str(obj.extract()?)),
        ValueType::Bool if is_bool => Some(Value::Bool(obj.extract()?)),
        _ => None,
    };
    value.ok_or_else(|| {
        let got = obj
            .get_type()
            .name()
            .map(|n| n.to_string())
            .unwrap_or_else(|_| "?".to_string());
        PyTypeError::new_err(format!("expected {}, got {got}", ty.name()))
    })
}

fn value_to_py<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    Ok(match value {
        Value::Nil => py.None().into_bound(py),
        Value::Float(f) => PyFloat::new(py, *f).into_any(),
        Value::Int(i) => i.into_pyobject(py)?.into_any(),
        Value::Str(s) => PyString::new(py, s).into_any(),
        Value::Bool(b) => PyBool::new(py, *b).to_owned().into_any(),
    })
}

fn make_callback(func: Py<PyAny>, ret: Option<ValueType>) -> Callback {
    Arc::new(move |args: &[Value]| -> Result<Value, CallbackError> {
        Python::attach(|py| {
            let args = args
                .iter()
                .map(|v| value_to_py(py, v))
                .collect::<PyResult<Vec<_>>>()?;
            let result = func.bind(py).call1(PyTuple::new(py, args)?)?;
            match ret {
                None => Ok(Value::Nil),
                Some(ty) => py_to_value(&result, ty),
            }
        })
        .map_err(|e: PyErr| Box::new(e) as CallbackError)
    })
}

fn check_callable(func: &Bound<'_, PyAny>) -> PyResult<()> {
    if func.is_callable() {
        Ok(())
    } else {
        Err(PyTypeError::new_err("func must be callable"))
    }
}

#[pymethods]
impl PyLuaRuntime {
    #[new]
    fn new() -> Self {
        Self {
            inner: LuaRuntime::new(),
        }
    }

    /// 変数を追加する。`name` はドット区切りで、途中のテーブルは自動で作られる。
    #[pyo3(signature = (
        name,
        value_type: "type[float] | type[int] | type[str] | type[bool]",
        value: "float | int | str | bool",
    ))]
    fn add_variable<'py>(
        mut slf: PyRefMut<'py, Self>,
        name: &str,
        value_type: &Bound<'py, PyAny>,
        value: &Bound<'py, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let ty = parse_value_type(value_type)?;
        let value = py_to_value(value, ty)?;
        slf.inner.add_variable(name, ty, value).map_err(to_pyerr)?;
        Ok(slf)
    }

    /// 関数を追加する。Lua から呼ばれるたびに `func` が呼ばれ、その戻り値が Lua に返る。
    /// `return_type` が `None` なら戻り値は `nil`。
    #[pyo3(signature = (
        name,
        func,
        return_type: "type[float] | type[int] | type[str] | type[bool] | None",
        *arg_types: "type[float] | type[int] | type[str] | type[bool]",
    ))]
    fn add_function<'py>(
        mut slf: PyRefMut<'py, Self>,
        name: &str,
        func: &Bound<'py, PyAny>,
        return_type: Option<&Bound<'py, PyAny>>,
        arg_types: &Bound<'py, PyTuple>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        check_callable(func)?;
        let ret = parse_return_type(return_type)?;
        let args = parse_arg_types(arg_types)?;
        let callback = make_callback(func.clone().unbind(), ret);
        slf.inner
            .add_function(name, callback, ret, args)
            .map_err(to_pyerr)?;
        Ok(slf)
    }

    /// プロシージャ(戻り値なし)を追加する。
    ///
    /// `realtime` が `True` なら呼ばれた時点で `func` を呼ぶ。`False` なら呼び出しを LuaJIT FFI の配列に積み、
    /// `run` でスクリプトが正常終了した後に呼び出し順通りに `func` を呼ぶ。
    #[pyo3(signature = (
        name,
        func,
        realtime,
        *arg_types: "type[float] | type[int] | type[str] | type[bool]",
    ))]
    fn add_procedure<'py>(
        mut slf: PyRefMut<'py, Self>,
        name: &str,
        func: &Bound<'py, PyAny>,
        realtime: bool,
        arg_types: &Bound<'py, PyTuple>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        check_callable(func)?;
        let args = parse_arg_types(arg_types)?;
        let callback = make_callback(func.clone().unbind(), None);
        slf.inner
            .add_procedure(name, callback, realtime, args)
            .map_err(to_pyerr)?;
        Ok(slf)
    }

    /// 登録済みの変数を更新する。
    #[pyo3(signature = (name, value: "float | int | str | bool"))]
    fn set_variable<'py>(
        mut slf: PyRefMut<'py, Self>,
        name: &str,
        value: &Bound<'py, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let ty = slf
            .inner
            .variable_type(name)
            .ok_or_else(|| to_pyerr(runtime::Error::UnknownVariable(name.to_string())))?;
        let value = py_to_value(value, ty).map_err(|e| {
            PyTypeError::new_err(format!("variable '{name}': {}", e.value(slf.py())))
        })?;
        slf.inner.set_variable(name, value).map_err(to_pyerr)?;
        Ok(slf)
    }

    /// スクリプトを実行し、遅延プロシージャを実行する。
    /// Lua のエラーは `RuntimeError`、登録した関数が投げた例外はそのまま送出される。
    fn run<'py>(mut slf: PyRefMut<'py, Self>, script: &str) -> PyResult<PyRefMut<'py, Self>> {
        slf.inner.run(script).map_err(to_pyerr)?;
        Ok(slf)
    }

    /// 登録済みの変数の現在値を返す。
    #[pyo3(signature = (name) -> "float | int | str | bool")]
    fn get_variable<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
        let value = self.inner.get_variable(name).map_err(to_pyerr)?;
        value_to_py(py, &value)
    }
}

#[pymodule(name = "lua_runtime")]
pub mod lua_runtime_module {
    #[pymodule_export]
    use super::PyLuaRuntime;

    #[pymodule_init]
    fn init(m: &pyo3::Bound<'_, pyo3::types::PyModule>) -> pyo3::PyResult<()> {
        use pyo3::types::PyModuleMethods;
        m.add("__version__", crate::VERSION)?;
        Ok(())
    }
}

use crate::Dirs;
use anyhow::Result;
use napi::bindgen_prelude::{FromNapiValue, Null, ToNapiValue};
use napi::sys;
use pyo3::prelude::*;
use pyo3::BoundObject;
use std::path::PathBuf;
use std::str::FromStr;

pub mod json_value;

/// JavaScript の `null | T` を表す型。
/// `Option<NullOr<T>>` とすることで undefined（None）/ null（Null）/ T（Value）の三値を表現できる。
#[derive(Clone, PartialEq, Debug)]
pub enum NullOr<T: Clone> {
    Null,
    Value(T),
}

impl<T: ToNapiValue + Clone> ToNapiValue for NullOr<T> {
    unsafe fn to_napi_value(env: sys::napi_env, val: Self) -> napi::Result<sys::napi_value> {
        match val {
            NullOr::Null => Null::to_napi_value(env, Null),
            NullOr::Value(t) => T::to_napi_value(env, t),
        }
    }
}

impl<T: FromNapiValue + Clone> FromNapiValue for NullOr<T> {
    unsafe fn from_napi_value(env: sys::napi_env, napi_val: sys::napi_value) -> napi::Result<Self> {
        // Null::from_napi_value は napi_null 以外でエラーを返すので、
        // これで null と他の型を区別できる。
        if Null::from_napi_value(env, napi_val).is_ok() {
            Ok(NullOr::Null)
        } else {
            T::from_napi_value(env, napi_val).map(NullOr::Value)
        }
    }
}

impl<'a, 'py, T: FromPyObject<'a, 'py> + Clone> FromPyObject<'a, 'py> for NullOr<T> {
    type Error = PyErr;

    fn extract(ob: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        if ob.is_none() {
            Ok(NullOr::Null)
        } else {
            T::extract(ob).map_err(Into::into).map(NullOr::Value)
        }
    }
}

impl<'py, T: IntoPyObject<'py> + Clone> IntoPyObject<'py> for NullOr<T>
where
    T::Error: Into<PyErr>,
{
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        match self {
            NullOr::Null => Ok(py.None().into_bound(py).into_any()),
            // as_any() は具体的な &Bound<'py, PyAny> を返すので associated type の問題を回避できる
            NullOr::Value(t) => t
                .into_pyobject(py)
                .map_err(Into::into)
                .map(|b| b.into_bound().into_any()),
        }
    }
}

pub fn get_data_dir(dirs: &Dirs) -> Result<PathBuf> {
    let appdata_dir = PathBuf::from_str(&dirs.data_dir)?;
    if !appdata_dir.exists() {
        println!("Creating app data directory at {:?}", &appdata_dir);
        std::fs::create_dir_all(&appdata_dir)?;
    }
    Ok(appdata_dir)
}

pub fn get_local_data_dir(dirs: &Dirs) -> Result<PathBuf> {
    let local_data_dir = PathBuf::from_str(&dirs.local_data_dir)?;
    if !local_data_dir.exists() {
        println!("Creating local data directory at {:?}", &local_data_dir);
        std::fs::create_dir_all(&local_data_dir)?;
    }
    Ok(local_data_dir)
}

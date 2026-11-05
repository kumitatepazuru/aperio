//! Python バインディング(`python` feature)。
//!
//! ```python
//! from lua_gpu import LuaGPU, CpuFunction, GpuFunction, PyFunction, Ext, Gen
//! gpu = LuaGPU([GpuFunction("obj.getpixel", gen, params=["float", "float"], returns=["float"] * 4), ...])
//! res = gpu.convert(source)          # ConvertResult | None(理由は gpu.last_diagnostics)
//! for k, params in res({"w": 1920, "h": 1080}):
//!     ...                            # res.shaders[k] を params で実行する
//! ```

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use mlua::{Lua, MultiValue, Value};
use pyo3::exceptions::{PyException, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyList, PyString, PyTuple};
use pyo3::IntoPyObjectExt;

use crate::diag::Diagnostic;
use crate::runtime::{ExtValue, RunError, Runtime};
use crate::spec::{
    CallSite, CodeGen, ConstValue, CpuFunction, ExternalSpec, FunctionSpec, Gen, GpuFunction,
    HostFn, Options, PyFunction, Registry,
};
use crate::types::TypeSpec;

pyo3::create_exception!(lua_gpu, LuaGpuError, PyException, "Base class of lua_gpu errors.");
pyo3::create_exception!(lua_gpu, LuaGpuRuntimeError, LuaGpuError, "The converted Lua failed at run time.");
pyo3::create_exception!(lua_gpu, LuaGpuArgError, LuaGpuError, "Invalid external values were passed to run().");

// ---- 生成関数とのやり取り ---------------------------------------------------------------

/// 生成関数の戻り値。1 つの式だけなら str を返してもよい。
#[pyclass(name = "Gen", module = "lua_gpu", from_py_object, get_all, set_all)]
#[derive(Clone, Default)]
pub struct PyGen {
    pub exprs: Vec<String>,
    pub stmts: String,
    pub defs: String,
}

#[pymethods]
impl PyGen {
    #[new]
    #[pyo3(signature = (exprs=None, stmts=String::new(), defs=String::new()))]
    fn new(exprs: Option<Vec<String>>, stmts: String, defs: String) -> Self {
        Self {
            exprs: exprs.unwrap_or_default(),
            stmts,
            defs,
        }
    }

    fn __repr__(&self) -> String {
        format!("Gen(exprs={:?}, stmts={:?}, defs={:?})", self.exprs, self.stmts, self.defs)
    }
}

/// 生成関数に渡される呼び出し箇所の情報。
#[pyclass(name = "CallSite", module = "lua_gpu", frozen)]
pub struct PyCallSite {
    site: CallSite,
}

fn const_to_py(py: Python<'_>, c: &Option<ConstValue>) -> PyResult<Py<PyAny>> {
    match c {
        None | Some(ConstValue::Nil) => Ok(py.None()),
        Some(ConstValue::Bool(b)) => b.into_py_any(py),
        Some(ConstValue::Number(n)) => n.into_py_any(py),
        Some(ConstValue::String(s)) => s.into_py_any(py),
    }
}

#[pymethods]
impl PyCallSite {
    /// spec の名前(`obj.getpixel`)。
    #[getter]
    fn name(&self) -> &str {
        &self.site.name
    }
    /// 対象言語での引数の式(仮引数名)。
    #[getter]
    fn args(&self) -> Vec<String> {
        self.site.args.clone()
    }
    /// 変換時に定数と分かった引数の値(分からなければ None)。
    #[getter]
    fn consts(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.site.consts.iter().map(|c| const_to_py(py, c)).collect()
    }
    #[getter]
    fn types(&self) -> Vec<String> {
        self.site.types.clone()
    }
    #[getter]
    fn is_method(&self) -> bool {
        self.site.is_method
    }
    #[getter]
    fn chunk(&self) -> &str {
        &self.site.chunk
    }
    #[getter]
    fn line(&self) -> u32 {
        self.site.line
    }
    #[getter]
    fn col(&self) -> u32 {
        self.site.col
    }
    /// "cpu"(Lua を返す)/ "kernel"(Slang を返す)。
    #[getter]
    fn context(&self) -> &'static str {
        self.site.context.as_str()
    }
    fn __repr__(&self) -> String {
        format!(
            "CallSite(name={:?}, args={:?}, types={:?}, {}:{}:{}, context={:?})",
            self.site.name,
            self.site.args,
            self.site.types,
            self.site.chunk,
            self.site.line,
            self.site.col,
            self.site.context.as_str()
        )
    }
}

struct PyGenerator(Py<PyAny>);

impl CodeGen for PyGenerator {
    fn generate(&self, site: &CallSite) -> Result<Option<Gen>, String> {
        Python::attach(|py| {
            let s = Py::new(py, PyCallSite { site: site.clone() }).map_err(|e| e.to_string())?;
            let r = self.0.call1(py, (s,)).map_err(|e| e.to_string())?;
            let r = r.bind(py);
            if r.is_none() {
                return Ok(None);
            }
            if let Ok(s) = r.cast::<PyString>() {
                return Ok(Some(Gen::expr(s.to_string())));
            }
            if let Ok(g) = r.cast::<PyGen>() {
                let g = g.borrow();
                return Ok(Some(Gen {
                    exprs: g.exprs.clone(),
                    stmts: g.stmts.clone(),
                    defs: g.defs.clone(),
                }));
            }
            if let Ok(list) = r.extract::<Vec<String>>() {
                return Ok(Some(Gen {
                    exprs: list,
                    ..Gen::default()
                }));
            }
            Err(format!(
                "generator must return str, Gen, list[str] or None (got {})",
                r.get_type().name().map(|n| n.to_string()).unwrap_or_default()
            ))
        })
    }
}

// ---- spec ------------------------------------------------------------------------------

/// 外部値の宣言(名前と型)。生成コードでは `${name}` で参照する。
#[pyclass(name = "Ext", module = "lua_gpu", from_py_object, get_all)]
#[derive(Clone)]
pub struct PyExt {
    pub name: String,
    pub r#type: String,
}

#[pymethods]
impl PyExt {
    #[new]
    fn new(name: String, r#type: String) -> Self {
        Self { name, r#type }
    }
    fn __repr__(&self) -> String {
        format!("Ext({:?}, {:?})", self.name, self.r#type)
    }
}

/// 変換時に Lua のコードを生成する独自関数。
#[pyclass(name = "CpuFunction", module = "lua_gpu", frozen)]
pub struct PyCpuFunction {
    name: String,
    generate: Py<PyAny>,
    returns: Option<Vec<String>>,
    externals: Vec<PyExt>,
    property: bool,
}

#[pymethods]
impl PyCpuFunction {
    #[new]
    #[pyo3(signature = (name, generate, *, returns=None, externals=None, property=false))]
    fn new(
        name: String,
        generate: Py<PyAny>,
        returns: Option<Vec<String>>,
        externals: Option<Vec<PyExt>>,
        property: bool,
    ) -> Self {
        Self {
            name,
            generate,
            returns,
            externals: externals.unwrap_or_default(),
            property,
        }
    }
}

/// 変換時に Slang のコードを生成する独自関数。
#[pyclass(name = "GpuFunction", module = "lua_gpu", frozen)]
pub struct PyGpuFunction {
    name: String,
    generate: Py<PyAny>,
    params: Vec<String>,
    returns: Vec<String>,
    externals: Vec<PyExt>,
    property: bool,
}

#[pymethods]
impl PyGpuFunction {
    #[new]
    #[pyo3(signature = (name, generate, *, params=None, returns=None, externals=None, property=false))]
    fn new(
        name: String,
        generate: Py<PyAny>,
        params: Option<Vec<String>>,
        returns: Option<Vec<String>>,
        externals: Option<Vec<PyExt>>,
        property: bool,
    ) -> Self {
        Self {
            name,
            generate,
            params: params.unwrap_or_default(),
            returns: returns.unwrap_or_default(),
            externals: externals.unwrap_or_default(),
            property,
        }
    }
}

/// 実行時に Lua から直接呼ばれるホスト関数(CPU 専用)。
#[pyclass(name = "PyFunction", module = "lua_gpu", frozen)]
pub struct PyPyFunction {
    name: String,
    func: Py<PyAny>,
    returns: Option<Vec<String>>,
}

#[pymethods]
impl PyPyFunction {
    #[new]
    #[pyo3(signature = (name, func, *, returns=None))]
    fn new(name: String, func: Py<PyAny>, returns: Option<Vec<String>>) -> Self {
        Self { name, func, returns }
    }
}

fn parse_type(s: &str) -> PyResult<TypeSpec> {
    s.parse::<TypeSpec>()
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

fn parse_types(list: &[String]) -> PyResult<Vec<TypeSpec>> {
    list.iter().map(|s| parse_type(s)).collect()
}

fn parse_exts(list: &[PyExt]) -> PyResult<Vec<ExternalSpec>> {
    list.iter()
        .map(|e| Ok(ExternalSpec::new(e.name.clone(), parse_type(&e.r#type)?)))
        .collect()
}

// ---- Lua ⇔ Python の値 -------------------------------------------------------------------

fn lua_to_py(py: Python<'_>, v: &Value, depth: u32) -> PyResult<Py<PyAny>> {
    if depth > 64 {
        return Err(PyValueError::new_err("table nesting is too deep"));
    }
    Ok(match v {
        Value::Nil => py.None(),
        Value::Boolean(b) => b.into_py_any(py)?,
        Value::Integer(i) => i.into_py_any(py)?,
        Value::Number(n) => n.into_py_any(py)?,
        Value::String(s) => s.to_string_lossy().into_py_any(py)?,
        Value::Table(t) => {
            let n = t.raw_len();
            let count = t.pairs::<Value, Value>().count();
            if n > 0 && n == count {
                let items: PyResult<Vec<Py<PyAny>>> = (1..=n)
                    .map(|i| {
                        let x: Value = t.raw_get(i).map_err(|e| PyValueError::new_err(e.to_string()))?;
                        lua_to_py(py, &x, depth + 1)
                    })
                    .collect();
                PyList::new(py, items?)?.into_any().unbind()
            } else {
                let d = PyDict::new(py);
                for pair in t.pairs::<Value, Value>() {
                    let (k, x) = pair.map_err(|e| PyValueError::new_err(e.to_string()))?;
                    d.set_item(lua_to_py(py, &k, depth + 1)?, lua_to_py(py, &x, depth + 1)?)?;
                }
                d.into_any().unbind()
            }
        }
        other => {
            return Err(PyTypeError::new_err(format!(
                "cannot pass a Lua {} to Python",
                other.type_name()
            )))
        }
    })
}

fn py_to_lua(lua: &Lua, v: &Bound<'_, PyAny>, depth: u32) -> mlua::Result<Value> {
    if depth > 64 {
        return Err(mlua::Error::RuntimeError("value nesting is too deep".into()));
    }
    if v.is_none() {
        return Ok(Value::Nil);
    }
    if let Ok(b) = v.cast::<PyBool>() {
        return Ok(Value::Boolean(b.is_true()));
    }
    if let Ok(f) = v.extract::<f64>() {
        return Ok(Value::Number(f));
    }
    if let Ok(s) = v.cast::<PyString>() {
        return Ok(Value::String(lua.create_string(s.to_string())?));
    }
    if let Ok(b) = v.cast::<PyBytes>() {
        return Ok(Value::String(lua.create_string(b.as_bytes())?));
    }
    if let Ok(d) = v.cast::<PyDict>() {
        let t = lua.create_table()?;
        for (k, x) in d.iter() {
            t.raw_set(py_to_lua(lua, &k, depth + 1)?, py_to_lua(lua, &x, depth + 1)?)?;
        }
        return Ok(Value::Table(t));
    }
    if let Ok(iter) = v.try_iter() {
        let t = lua.create_table()?;
        for (i, x) in iter.enumerate() {
            let x = x.map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            t.raw_set(i + 1, py_to_lua(lua, &x, depth + 1)?)?;
        }
        return Ok(Value::Table(t));
    }
    Err(mlua::Error::RuntimeError(format!(
        "cannot pass a Python {} to Lua",
        v.get_type().name().map(|n| n.to_string()).unwrap_or_default()
    )))
}

fn host_fn(func: Py<PyAny>, name: String) -> HostFn {
    Arc::new(move |lua: &Lua, args: MultiValue| {
        Python::attach(|py| {
            let pyargs: Vec<Py<PyAny>> = args
                .iter()
                .map(|v| lua_to_py(py, v, 0))
                .collect::<PyResult<_>>()
                .map_err(|e| mlua::Error::RuntimeError(format!("{name}: {e}")))?;
            let tuple = PyTuple::new(py, pyargs)
                .map_err(|e| mlua::Error::RuntimeError(format!("{name}: {e}")))?;
            let r = func
                .call1(py, tuple)
                .map_err(|e| mlua::Error::RuntimeError(format!("{name}: {e}")))?;
            let r = r.bind(py);
            // tuple は多値の戻り値。
            if let Ok(t) = r.cast::<PyTuple>() {
                let mut out = MultiValue::new();
                for x in t.iter() {
                    out.push_back(py_to_lua(lua, &x, 0)?);
                }
                return Ok(out);
            }
            let mut out = MultiValue::new();
            out.push_back(py_to_lua(lua, r, 0)?);
            Ok(out)
        })
    })
}

fn py_to_ext(v: &Bound<'_, PyAny>, ty: &TypeSpec, path: &str) -> PyResult<ExtValue> {
    let err = |want: &str| LuaGpuArgError::new_err(format!("external `{path}` must be {want}"));
    match ty {
        TypeSpec::Float | TypeSpec::Int => {
            if v.cast::<PyBool>().is_ok() {
                return Err(err("a number"));
            }
            v.extract::<f64>().map(ExtValue::Number).map_err(|_| err("a number"))
        }
        TypeSpec::Bool => v
            .cast::<PyBool>()
            .map(|b| ExtValue::Bool(b.is_true()))
            .map_err(|_| err("a bool")),
        TypeSpec::String => v.extract::<String>().map(ExtValue::String).map_err(|_| err("a str")),
        TypeSpec::Any => any_to_ext(v, path),
        TypeSpec::Array(elem, _) => {
            if v.cast::<PyString>().is_ok() || v.cast::<PyDict>().is_ok() {
                return Err(err("a list"));
            }
            let iter = v.try_iter().map_err(|_| err("a list"))?;
            let mut items = Vec::new();
            for (i, x) in iter.enumerate() {
                items.push(py_to_ext(&x?, elem, &format!("{path}[{}]", i + 1))?);
            }
            Ok(ExtValue::Array(items))
        }
        TypeSpec::Record(fields) => {
            let d = v.cast::<PyDict>().map_err(|_| err("a dict"))?;
            let mut out = IndexMap::new();
            for (k, t) in fields {
                let x = d
                    .get_item(k)?
                    .ok_or_else(|| LuaGpuArgError::new_err(format!("external `{path}` is missing field `{k}`")))?;
                out.insert(k.clone(), py_to_ext(&x, t, &format!("{path}.{k}"))?);
            }
            Ok(ExtValue::Record(out))
        }
    }
}

fn any_to_ext(v: &Bound<'_, PyAny>, path: &str) -> PyResult<ExtValue> {
    if let Ok(b) = v.cast::<PyBool>() {
        return Ok(ExtValue::Bool(b.is_true()));
    }
    if let Ok(f) = v.extract::<f64>() {
        return Ok(ExtValue::Number(f));
    }
    if let Ok(s) = v.extract::<String>() {
        return Ok(ExtValue::String(s));
    }
    if let Ok(d) = v.cast::<PyDict>() {
        let mut out = IndexMap::new();
        for (k, x) in d.iter() {
            let k: String = k.extract()?;
            let x = any_to_ext(&x, &format!("{path}.{k}"))?;
            out.insert(k, x);
        }
        return Ok(ExtValue::Record(out));
    }
    if let Ok(iter) = v.try_iter() {
        let mut items = Vec::new();
        for (i, x) in iter.enumerate() {
            items.push(any_to_ext(&x?, &format!("{path}[{}]", i + 1))?);
        }
        return Ok(ExtValue::Array(items));
    }
    Err(LuaGpuArgError::new_err(format!("external `{path}` has an unsupported type")))
}

fn diag_to_py(py: Python<'_>, d: &Diagnostic) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("severity", d.severity.as_str())?;
    dict.set_item("code", d.code)?;
    dict.set_item("message", &d.message)?;
    dict.set_item("chunk", &d.location.chunk)?;
    dict.set_item("line", d.location.line)?;
    dict.set_item("col", d.location.col)?;
    Ok(dict.into_any().unbind())
}

// ---- LuaGPU ----------------------------------------------------------------------------

/// Lua → Slang 変換器。
#[pyclass(name = "LuaGPU", module = "lua_gpu", frozen)]
pub struct PyLuaGpu {
    registry: Registry,
    runtime: Runtime,
    last: Mutex<Vec<Diagnostic>>,
}

#[pymethods]
impl PyLuaGpu {
    #[new]
    #[pyo3(signature = (functions, *, package_path=None, max_loop_iterations=1_000_000, max_values_per_iteration=16, max_local_table_len=64, extra_inputs=None))]
    fn new(
        functions: Vec<Bound<'_, PyAny>>,
        package_path: Option<String>,
        max_loop_iterations: u32,
        max_values_per_iteration: u32,
        max_local_table_len: u32,
        extra_inputs: Option<Vec<String>>,
    ) -> PyResult<Self> {
        let mut specs = Vec::new();
        for f in &functions {
            if let Ok(c) = f.cast::<PyCpuFunction>() {
                let c = c.get();
                specs.push(FunctionSpec::Cpu(CpuFunction {
                    name: c.name.clone(),
                    generate: Arc::new(PyGenerator(c.generate.clone_ref(f.py()))),
                    returns: c.returns.as_deref().map(parse_types).transpose()?,
                    externals: parse_exts(&c.externals)?,
                    property: c.property,
                }));
            } else if let Ok(g) = f.cast::<PyGpuFunction>() {
                let g = g.get();
                specs.push(FunctionSpec::Gpu(GpuFunction {
                    name: g.name.clone(),
                    generate: Arc::new(PyGenerator(g.generate.clone_ref(f.py()))),
                    params: parse_types(&g.params)?,
                    returns: parse_types(&g.returns)?,
                    externals: parse_exts(&g.externals)?,
                    property: g.property,
                }));
            } else if let Ok(p) = f.cast::<PyPyFunction>() {
                let p = p.get();
                specs.push(FunctionSpec::Py(PyFunction {
                    name: p.name.clone(),
                    func: host_fn(p.func.clone_ref(f.py()), p.name.clone()),
                    returns: p.returns.as_deref().map(parse_types).transpose()?,
                }));
            } else {
                return Err(PyTypeError::new_err(
                    "functions must be CpuFunction, GpuFunction or PyFunction",
                ));
            }
        }
        let mut options = Options {
            max_loop_iterations,
            max_values_per_iteration,
            max_local_table_len,
            extra_inputs: extra_inputs.unwrap_or_default(),
            ..Options::default()
        };
        if let Some(p) = package_path {
            options.package_path = p;
        }
        let registry =
            Registry::new(specs, options).map_err(|e| PyValueError::new_err(e.to_string()))?;
        let runtime = Runtime::new(&registry).map_err(|e| LuaGpuError::new_err(e.to_string()))?;
        Ok(Self {
            registry,
            runtime,
            last: Mutex::new(Vec::new()),
        })
    }

    /// 変換する。失敗したら None(理由は last_diagnostics)。
    #[pyo3(signature = (source, chunk_name="main.lua", base_dir=None))]
    fn convert(
        slf: &Bound<'_, Self>,
        source: &str,
        chunk_name: &str,
        base_dir: Option<PathBuf>,
    ) -> PyResult<Option<PyConvertResult>> {
        let py = slf.py();
        let this = slf.get();
        let out = py.detach(|| crate::convert(source, chunk_name, base_dir.as_deref(), &this.registry));
        *this.last.lock().unwrap() = out.diagnostics;
        Ok(out.result.map(|c| PyConvertResult {
            lua: c.lua,
            shaders: c.shaders,
            externals: c
                .externals
                .into_iter()
                .map(|(k, t)| (k, t.to_string()))
                .collect(),
            owner: slf.clone().unbind(),
        }))
    }

    /// CPU 用 Lua を実行し、ディスパッチ列 `[(kernel_index, params_bytes)]` を返す。
    /// `on_dispatch` を渡すと、ディスパッチごとに実行順でそれを呼ぶ(戻り値は空のリスト)。
    #[pyo3(signature = (lua, externals=None, on_dispatch=None))]
    fn run(
        &self,
        py: Python<'_>,
        lua: &str,
        externals: Option<Bound<'_, PyDict>>,
        on_dispatch: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyList>> {
        let mut ext = IndexMap::new();
        if let Some(d) = &externals {
            for (k, v) in d.iter() {
                let name: String = k.extract()?;
                let ty = self.runtime.externals().get(&name).ok_or_else(|| {
                    LuaGpuArgError::new_err(format!("unknown external value `{name}`"))
                })?;
                ext.insert(name.clone(), py_to_ext(&v, ty, &name)?);
            }
        }
        let mut collected: Vec<(u32, Vec<u8>)> = Vec::new();
        let result = py.detach(|| {
            self.runtime.run(lua, &ext, &mut |k, bytes| match &on_dispatch {
                Some(cb) => Python::attach(|py| {
                    cb.call1(py, (k, PyBytes::new(py, &bytes)))
                        .map(|_| ())
                        .map_err(|e| format!("on_dispatch: {e}"))
                }),
                None => {
                    collected.push((k, bytes));
                    Ok(())
                }
            })
        });
        match result {
            Ok(()) => {
                let items: PyResult<Vec<Py<PyAny>>> = collected
                    .into_iter()
                    .map(|(k, b)| (k, PyBytes::new(py, &b)).into_py_any(py))
                    .collect();
                Ok(PyList::new(py, items?)?.unbind())
            }
            Err(RunError::Arg(m)) => Err(LuaGpuArgError::new_err(m)),
            Err(RunError::Layout(m)) | Err(RunError::Lua(m)) => Err(LuaGpuRuntimeError::new_err(m)),
        }
    }

    /// 直前の convert の診断(エラー・警告)。
    #[getter]
    fn last_diagnostics(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.last
            .lock()
            .unwrap()
            .iter()
            .map(|d| diag_to_py(py, d))
            .collect()
    }

    /// 外部値の宣言 `[(name, type)]`。
    #[getter]
    fn externals(&self) -> Vec<(String, String)> {
        self.registry
            .externals
            .iter()
            .map(|(k, t)| (k.clone(), t.to_string()))
            .collect()
    }
}

/// 変換結果。呼び出すと run と同じ。
#[pyclass(name = "ConvertResult", module = "lua_gpu", frozen)]
pub struct PyConvertResult {
    #[pyo3(get)]
    lua: String,
    #[pyo3(get)]
    shaders: Vec<String>,
    #[pyo3(get)]
    externals: Vec<(String, String)>,
    owner: Py<PyLuaGpu>,
}

#[pymethods]
impl PyConvertResult {
    #[pyo3(signature = (externals=None, on_dispatch=None))]
    fn __call__(
        &self,
        py: Python<'_>,
        externals: Option<Bound<'_, PyDict>>,
        on_dispatch: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyList>> {
        self.owner.get().run(py, &self.lua, externals, on_dispatch)
    }

    fn __repr__(&self) -> String {
        format!("ConvertResult(kernels={})", self.shaders.len())
    }
}

#[pymodule(name = "lua_gpu")]
pub mod lua_gpu_module {
    #[pymodule_export]
    use super::{
        PyCallSite, PyConvertResult, PyCpuFunction, PyExt, PyGen, PyGpuFunction, PyLuaGpu,
        PyPyFunction,
    };

    #[pymodule_init]
    fn init(m: &pyo3::Bound<'_, pyo3::types::PyModule>) -> pyo3::PyResult<()> {
        use pyo3::types::PyModuleMethods;
        let py = m.py();
        m.add("__version__", crate::VERSION)?;
        m.add("LuaGpuError", py.get_type::<super::LuaGpuError>())?;
        m.add("LuaGpuRuntimeError", py.get_type::<super::LuaGpuRuntimeError>())?;
        m.add("LuaGpuArgError", py.get_type::<super::LuaGpuArgError>())?;
        Ok(())
    }
}

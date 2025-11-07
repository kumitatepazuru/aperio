use std::time::Instant;

use anyhow::Result;
use numpy::{PyArray1, PyReadonlyArray1, ToPyArray};
use pyo3::{prelude::*, types::*};
use pyo3_stub_gen::{
    define_stub_info_gatherer,
    derive::{gen_stub_pyclass, gen_stub_pymethods},
};
use tokio::runtime::Runtime;

use crate::{
    compiled_func::{CpuFunction, CpuInputImage, CpuOutput},
    image_generate_builder::ImageGenerateBuilder,
};

pub mod compiled_func;
pub mod compiled_wgsl;
pub mod image_generate_builder;
pub mod image_generator;

// Pythonで動かすためのライブラリのラッパーを作る
#[gen_stub_pyclass]
#[pyclass]
pub struct PyCompiledWgsl {
    pub inner: compiled_wgsl::CompiledWgsl,
}

#[gen_stub_pyclass]
#[pyclass]
pub struct PyCompiledFunc {
    _id: String,
    pub inner: compiled_func::CompiledFunc,
}

#[gen_stub_pyclass]
#[pyclass]
#[derive(Clone)]
pub struct PyImageGenerateBuilder {
    pub inner: image_generate_builder::ImageGenerateBuilder,
}

#[gen_stub_pyclass]
#[pyclass]
pub struct PyImageGenerator {
    pub inner: image_generator::ImageGenerator,
    rt: Runtime,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyCompiledWgsl {
    #[new]
    pub fn new(id: &str, wgsl_code: &str, generator: &PyImageGenerator) -> Result<Self, PyErr> {
        let inner = compiled_wgsl::CompiledWgsl::new(id, wgsl_code, &generator.inner.device)?;

        Ok(Self { inner })
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyCompiledFunc {
    #[new]
    pub fn new(id: &str, func: Py<PyAny>) -> PyResult<Self> {
        let func_ref = func;
        let func: Box<CpuFunction> =
            Box::new(move |data: &[CpuInputImage], params: Option<&[u8]>| {
                Python::attach(|py| {
                    let pickle = py.import("pickle")?;
                    let pickle_loads = pickle.getattr("loads")?;

                    // パラメータをPythonの型に変換
                    let py_data = data
                        .iter()
                        .map(|n| {
                            let py_data = PyDict::new(py);
                            py_data.set_item("data", n.data.to_pyarray(py))?;
                            py_data.set_item("width", n.width)?;
                            py_data.set_item("height", n.height)?;
                            Ok(py_data)
                        })
                        .collect::<PyResult<Vec<_>>>()?;
                    let py_data = PyList::new(py, py_data)?;

                    let py_params = if let Some(param_bytes) = params {
                        Some(pickle_loads.call1((param_bytes,))?)
                    } else {
                        None
                    };

                    // { data: ndarray, width: int, height: int }
                    let output = func_ref.call1(py, (py_data, py_params))?;
                    let output = output.bind(py);
                    let out_data: PyReadonlyArray1<f32> = output
                        .getattr("data")?
                        .extract()
                        .map_err(|e: pyo3::CastError<'_, '_>| anyhow::anyhow!(e.to_string()))?;

                    // 全部変換
                    let output = CpuOutput {
                        data: out_data.as_slice()?.to_vec(),
                        width: output.getattr("width")?.extract()?,
                        height: output.getattr("height")?.extract()?,
                    };
                    Ok(output)
                })
            });

        let inner = compiled_func::CompiledFunc::new(func);

        Ok(Self {
            _id: id.to_string(),
            inner,
        })
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyImageGenerateBuilder {
    #[new]
    pub fn new() -> Self {
        let inner = ImageGenerateBuilder::new();

        Self { inner }
    }

    pub fn add_wgsl<'py>(
        &self,
        wgsl: &PyCompiledWgsl,
        params: Option<&Bound<'py, PyBytes>>,
        output_width: u32,
        output_height: u32,
    ) -> Self {
        let params = params.map(|p| p.as_bytes().to_vec());

        let new_inner =
            self.inner
                .clone()
                .add_wgsl(wgsl.inner.clone(), params, output_width, output_height);

        Self { inner: new_inner }
    }

    pub fn add_parallel_wgsl<'py>(
        &self,
        py: Python<'py>,
        pipelines: Vec<Py<PyImageGenerateBuilder>>,
    ) -> PyResult<Self> {
        let pipelines: Result<Vec<ImageGenerateBuilder>, PyErr> = pipelines
            .into_iter()
            .map(|n| {
                let builder = n.borrow(py);
                Ok(builder.inner.clone())
            })
            .collect();
        let pipelines = pipelines?;
        let new_inner = self.inner.clone().add_parallel_wgsl(pipelines);

        Ok(Self { inner: new_inner })
    }

    pub fn add_func<'py>(
        &self,
        py: Python<'py>,
        func: &PyCompiledFunc,
        params: Option<Py<PyAny>>,
        output_width: u32,
        output_height: u32,
    ) -> PyResult<Self> {
        let params = if let Some(p) = params {
            let pickle = py.import("pickle")?;
            let pickle_dumps = pickle.getattr("dumps")?;
            let dumped: Py<PyAny> = pickle_dumps.call1((p,))?.unbind();
            let dumped = dumped.bind(py);
            let dumped: Vec<u8> = dumped.extract()?;
            Some(dumped)
        } else {
            None
        };

        let new_inner =
            self.inner
                .clone()
                .add_func(func.inner.clone(), params, output_width, output_height);

        Ok(Self { inner: new_inner })
    }
}

#[gen_stub_pymethods]
#[pymethods]
// TODO: experimental-asyncを使った非同期処理
impl PyImageGenerator {
    #[new]
    pub fn new() -> Result<Self> {
        let rt = Runtime::new()?;
        let inner = rt.block_on(async { image_generator::ImageGenerator::new().await })?;
        Ok(Self { inner, rt })
    }

    pub fn generate<'py>(
        &self,
        py: Python<'py>,
        builder: &PyImageGenerateBuilder,
    ) -> PyResult<Bound<'py, PyArray1<u8>>> {
        let result = self
            .rt
            .block_on(async { self.inner.generate(builder.inner.clone()).await })?;

        let time = Instant::now();
        let b = result.to_pyarray(py);
        println!(
            "PyImageGenerator::generate: Finished generation in {:?}",
            time.elapsed()
        );
        Ok(b)
    }
}

#[pymodule]
pub fn gpu_util(m: &Bound<PyModule>) -> PyResult<()> {
    println!("gpu_util: Initializing gpu_util module");
    m.add_class::<PyCompiledWgsl>()?;
    m.add_class::<PyCompiledFunc>()?;
    m.add_class::<PyImageGenerateBuilder>()?;
    m.add_class::<PyImageGenerator>()?;
    Ok(())
}

define_stub_info_gatherer!(stub_info);

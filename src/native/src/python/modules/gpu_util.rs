use anyhow::Result;
use napi_derive::napi;
use numpy::{PyReadonlyArray1, ToPyArray};
use pyo3::{exceptions::PyValueError, prelude::*, types::*};
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;

// gpu_util crate の純粋 Rust 型をインポート（ローカルモジュール名と衝突しないよう明示的に)
use gpu_util::compiled_func::{
    self, CpuFunction, CpuInputImage, CpuOutput, GpuInputTexture, GpuTextureOutput, TextureFunction,
};
use gpu_util::image_generate_builder::ImageGenerateBuilder;
use gpu_util::image_generator;
use gpu_util::{compiled_wgsl, image_generate_builder, SharedTextureFormat};

use crate::python::modules::compose_wgsl::{
    PyComposableModuleDescriptor, PyNagaModuleDescriptor,
};

#[cfg(target_os = "linux")]
use gpu_util::texture_to_native::linux::SharedTextureHandle;
#[cfg(target_os = "windows")]
use gpu_util::texture_to_native::windows::SharedTextureHandle;

// texture formatをenumで定義
#[napi]
#[pyclass(from_py_object, eq)]
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum WrappedSharedTextureFormat {
    Rgba16Float,
    Bgra8Unorm,
}

// Pythonで動かすためのライブラリのラッパーを作る
#[pyclass]
pub struct PySamplerOptions {
    pub inner: compiled_wgsl::SamplerOptions,
}

#[pyclass]
pub struct PyCompiledWgsl {
    pub inner: compiled_wgsl::CompiledWgsl,
}

#[pyclass]
pub struct PyCompiledFunc {
    _id: String,
    py_callback: Py<PyAny>,
}

/// wgpu::Texture のPythonラッパー。
/// Pythonから直接生成することはできず、TextureFunc の引数・戻り値として使用する。
#[pyclass]
pub struct PyTexture {
    pub inner: std::sync::Arc<wgpu::Texture>,
    pub width: u32,
    pub height: u32,
}

#[pyclass]
pub struct PyCompiledTextureFunc {
    _id: String,
    py_callback: Py<PyAny>,
}

#[pyclass]
pub struct PySharedTextureHandle {
    pub inner: SharedTextureHandle,
}

#[pyclass(from_py_object)]
#[derive(Clone)]
pub struct PyImageGenerateBuilder {
    pub inner: image_generate_builder::ImageGenerateBuilder,
}

#[pyclass]
pub struct PyImageGenerator {
    pub inner: image_generator::ImageGenerator,
    rt: Runtime,
}

fn make_cpu_func(
    py_callback: Py<PyAny>,
    py_params: Option<Py<PyAny>>,
) -> compiled_func::CompiledFunc {
    let func: Box<CpuFunction> = Box::new(move |data: &[CpuInputImage], _: Option<&[u8]>| {
        Python::attach(|py| {
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

            let py_p = py_params.as_ref().map(|p| p.bind(py));

            // ndarray (f32) を直接返す
            let output = py_callback.call1(py, (py_data, py_p))?;
            let out_data: PyReadonlyArray1<f32> = output
                .bind(py)
                .extract()
                .map_err(|e: pyo3::CastError<'_, '_>| anyhow::anyhow!(e.to_string()))?;

            Ok(CpuOutput {
                data: out_data.as_slice()?.to_vec(),
            })
        })
    });
    compiled_func::CompiledFunc::new(func)
}

fn make_texture_func(
    py_callback: Py<PyAny>,
    py_params: Option<Py<PyAny>>,
) -> compiled_func::CompiledTextureFunc {
    let func: Box<TextureFunction> =
        Box::new(move |inputs: Vec<GpuInputTexture>, _: Option<&[u8]>| {
            Python::attach(|py| {
                // Vec<GpuInputTexture> を PyTexture のリストに変換
                let py_inputs = inputs
                    .into_iter()
                    .map(|input| {
                        Py::new(
                            py,
                            PyTexture {
                                inner: input.texture,
                                width: input.width,
                                height: input.height,
                            },
                        )
                        .map_err(|e| anyhow::anyhow!(e.to_string()))
                    })
                    .collect::<anyhow::Result<Vec<Py<PyTexture>>>>()?;

                let py_p = py_params.as_ref().map(|p| p.bind(py));

                // PyTexture を直接返す
                let output = py_callback.call1(py, (py_inputs, py_p))?;
                let py_texture = output
                    .bind(py)
                    .cast::<PyTexture>()
                    .map_err(|e| anyhow::anyhow!("Expected PyTexture: {}", e))?;
                let inner_texture = py_texture.borrow().inner.clone();

                Ok(GpuTextureOutput {
                    texture: inner_texture,
                })
            })
        });
    compiled_func::CompiledTextureFunc::new(func)
}

#[pymethods]
impl PySamplerOptions {
    #[new]
    pub fn new(address_mode: &str, filter: &str) -> PyResult<Self> {
        let address_mode = match address_mode {
            "clamp_to_edge" => wgpu::AddressMode::ClampToEdge,
            "repeat" => wgpu::AddressMode::Repeat,
            "mirror_repeat" => wgpu::AddressMode::MirrorRepeat,
            "clamp_to_border" => wgpu::AddressMode::ClampToBorder,
            _ => {
                return Err(PyValueError::new_err(
                    "Invalid address_mode. Must be one of: clamp_to_edge, repeat, mirror_repeat, clamp_to_border",
                ));
            }
        };

        let filter = match filter {
            "nearest" => wgpu::FilterMode::Nearest,
            "linear" => wgpu::FilterMode::Linear,
            _ => {
                return Err(PyValueError::new_err(
                    "Invalid filter. Must be one of: nearest, linear",
                ));
            }
        };

        Ok(Self {
            inner: compiled_wgsl::SamplerOptions {
                address_mode,
                filter,
            },
        })
    }
}

#[pymethods]
impl PyCompiledWgsl {
    #[new]
    pub fn new(
        id: &str,
        wgsl_code: &str,
        generator: &PyImageGenerator,
        sampler_options: Option<&PySamplerOptions>,
    ) -> Result<Self, PyErr> {
        let inner = compiled_wgsl::CompiledWgsl::new(
            id,
            wgsl_code,
            &generator.inner.device,
            sampler_options.map(|s| &s.inner),
        )?;

        Ok(Self { inner })
    }

    /// composable module 群を naga_oil で合成し、その naga IR からシェーダーを作る。
    ///
    /// `id` はパイプラインキャッシュのキーにもなるため、同じシェーダーを異なる
    /// `shader_defs` で合成する場合は必ず別の `id` を渡すこと。
    #[staticmethod]
    #[pyo3(signature = (id, composable_modules, naga_module, generator, sampler_options=None))]
    pub fn compose_new(
        id: &str,
        composable_modules: Vec<PyRef<'_, PyComposableModuleDescriptor>>,
        naga_module: &PyNagaModuleDescriptor,
        generator: &PyImageGenerator,
        sampler_options: Option<&PySamplerOptions>,
    ) -> Result<Self, PyErr> {
        let composable_modules: Vec<_> = composable_modules.iter().map(|m| &m.inner).collect();

        let inner = compiled_wgsl::CompiledWgsl::compose_new(
            id,
            &composable_modules,
            &naga_module.inner,
            &generator.inner.device,
            sampler_options.map(|s| &s.inner),
        )?;

        Ok(Self { inner })
    }
}

#[pymethods]
impl PyCompiledFunc {
    #[new]
    pub fn new(id: &str, func: Py<PyAny>) -> PyResult<Self> {
        Ok(Self {
            _id: id.to_string(),
            py_callback: func,
        })
    }
}

#[pymethods]
impl PyTexture {
    #[getter]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[getter]
    pub fn height(&self) -> u32 {
        self.height
    }
}

#[pymethods]
impl PyCompiledTextureFunc {
    #[new]
    pub fn new(id: &str, func: Py<PyAny>) -> PyResult<Self> {
        Ok(Self {
            _id: id.to_string(),
            py_callback: func,
        })
    }
}

impl PySharedTextureHandle {
    pub fn new(handle: SharedTextureHandle) -> Self {
        Self { inner: handle }
    }
}

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

    pub fn add_builder(&self, _py: Python<'_>, other: &PyImageGenerateBuilder) -> Self {
        let new_inner = self.inner.clone().chain(&other.inner);
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
        let compiled = make_cpu_func(func.py_callback.clone_ref(py), params);
        let new_inner = self
            .inner
            .clone()
            .add_func(compiled, None, output_width, output_height);
        Ok(Self { inner: new_inner })
    }

    pub fn add_texture_func<'py>(
        &self,
        py: Python<'py>,
        func: &PyCompiledTextureFunc,
        params: Option<Py<PyAny>>,
        output_width: u32,
        output_height: u32,
    ) -> PyResult<Self> {
        let compiled = make_texture_func(func.py_callback.clone_ref(py), params);
        let new_inner =
            self.inner
                .clone()
                .add_texture_func(compiled, None, output_width, output_height);
        Ok(Self { inner: new_inner })
    }
}

#[pymethods]
// TODO: experimental-asyncを使った非同期処理
impl PyImageGenerator {
    #[new]
    pub fn new() -> Result<Self> {
        let rt = Runtime::new()?;
        let inner = rt.block_on(async { image_generator::ImageGenerator::new().await })?;
        Ok(Self { inner, rt })
    }

    /// このデバイスで確保できる2Dテクスチャの最大辺長（px）。
    /// キャンバスを広げるエフェクトが拡張量を切り詰める上限として使う。
    #[getter]
    pub fn maximum_texture_size(&self) -> u32 {
        self.inner.maximum_texture_size
    }

    pub fn generate_buf(
        &self,
        builder: &PyImageGenerateBuilder,
        buffer_ptr: usize,
    ) -> PyResult<()> {
        let result = self
            .rt
            .block_on(async { self.inner.generate_buf(builder.inner.clone()).await })?;

        // 直接メモリコピー
        unsafe {
            std::ptr::copy_nonoverlapping(result.as_ptr(), buffer_ptr as *mut u8, result.len());
        }

        Ok(())
    }

    pub fn generate_shared_texture(
        &self,
        builder: &PyImageGenerateBuilder,
        texture_handle: &PySharedTextureHandle,
        format: &WrappedSharedTextureFormat,
    ) -> PyResult<()> {
        let format = match format {
            WrappedSharedTextureFormat::Rgba16Float => SharedTextureFormat::Rgba16Float,
            WrappedSharedTextureFormat::Bgra8Unorm => SharedTextureFormat::Bgra8Unorm,
        };

        self.rt.block_on(async {
            self.inner
                .generate_shared_texture(builder.inner.clone(), &texture_handle.inner, &format)
                .await
        })?;

        Ok(())
    }
}

#[pymodule(name = "gpu_util")]
pub mod gpu_util_register {
    #[pymodule_export]
    use super::PyCompiledFunc;
    #[pymodule_export]
    use super::PyCompiledTextureFunc;
    #[pymodule_export]
    use super::PyCompiledWgsl;
    #[pymodule_export]
    use super::PyImageGenerateBuilder;
    #[pymodule_export]
    use super::PyImageGenerator;
    #[pymodule_export]
    use super::PySamplerOptions;
    #[pymodule_export]
    use super::PySharedTextureHandle;
    #[pymodule_export]
    use super::PyTexture;
    #[pymodule_export]
    use super::WrappedSharedTextureFormat;
    #[pymodule_export]
    use crate::python::modules::compose_wgsl::create_composable_module;
    #[pymodule_export]
    use crate::python::modules::compose_wgsl::create_naga_module;
    #[pymodule_export]
    use crate::python::modules::compose_wgsl::PyComposableModuleDescriptor;
    #[pymodule_export]
    use crate::python::modules::compose_wgsl::PyNagaModuleDescriptor;
}

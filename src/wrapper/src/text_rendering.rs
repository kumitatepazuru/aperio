use pyo3::{
    pyclass, pymethods, pymodule,
    types::{PyModule, PyModuleMethods},
    Bound, PyResult,
};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use text_rendering::TextRenderer;

#[gen_stub_pyclass]
#[pyclass(module = "aperio.text_rendering")]
pub struct PyTextRenderer {
    pub inner: TextRenderer,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyTextRenderer {
    #[new]
    pub fn new(device: &crate::gpu_util::PyImageGenerator) -> Self {
        Self {
            inner: TextRenderer::new(&device.inner),
        }
    }
}

#[pymodule]
pub fn text_rendering_register(m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<PyTextRenderer>()?;
    Ok(())
}

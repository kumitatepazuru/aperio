use pyo3::{
    pyclass, pymethods, pymodule,
    types::{PyModule, PyModuleMethods},
    Bound, Py, PyResult,
};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use text_rendering::{text_renderer::TextRenderer, TextSpec};

use crate::gpu_util::PyTexture;

#[gen_stub_pyclass]
#[pyclass(module = "aperio.text_rendering")]
pub struct PyTextRenderer {
    pub inner: TextRenderer,
}

#[gen_stub_pyclass]
#[pyclass(module = "aperio.text_rendering")]
pub struct PyTextSpec {
    pub inner: TextSpec,
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

    pub fn render_text(&mut self, spec: &PyTextSpec) -> PyResult<Option<PyTexture>> {
        let texture = self.inner.render_text(&spec.inner)?;

        Ok(texture.map(|tex| {
            let width = tex.width();
            let height = tex.height();
            PyTexture {
                inner: tex,
                width,
                height,
            }
        }))
    }

    pub fn render_text_for_pipeline(
        &mut self,
        _inputs: Vec<Py<PyTexture>>,
        spec: &PyTextSpec,
    ) -> PyResult<Option<PyTexture>> {
        self.render_text(spec)
    }

    pub fn render_chars(&mut self, spec: &PyTextSpec) -> PyResult<Vec<(char, PyTexture)>> {
        let textures = self.inner.render_chars(&spec.inner)?;

        Ok(textures
            .into_iter()
            .map(|(ch, tex)| {
                let width = tex.width();
                let height = tex.height();
                (
                    ch,
                    PyTexture {
                        inner: tex,
                        width,
                        height,
                    },
                )
            })
            .collect())
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyTextSpec {
    #[new]
    pub fn new(
        text: String,
        font_size: f32,
        color: [u8; 4],
        font_family: Option<String>,
        max_width: Option<u32>,
        line_spacing: f32,
        char_spacing: f32,
    ) -> Self {
        Self {
            inner: TextSpec {
                text,
                font_size,
                color,
                font_family: font_family,
                max_width: max_width,
                line_spacing: line_spacing,
                char_spacing: char_spacing,
            },
        }
    }
}

#[pymodule]
pub fn text_rendering_register(m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<PyTextRenderer>()?;
    m.add_class::<PyTextSpec>()?;
    Ok(())
}

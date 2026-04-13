use pyo3::{
    pyclass, pymethods, pymodule,
    types::{PyModule, PyModuleMethods},
    Bound, Py, PyResult,
};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use text_rendering::{text_renderer::TextRenderer, CharGlyphData, PreparedText, TextSpec};

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

/// `prepare_render_text` の戻り値。グリフデータを保持し、`run_render_text` / `run_render_chars` に渡す。
#[gen_stub_pyclass]
#[pyclass(module = "aperio.text_rendering")]
pub struct PyPreparedText {
    pub inner: PreparedText,
}

/// 1 文字分のグリフ位置情報。`run_render_chars` の戻り値要素。
#[gen_stub_pyclass]
#[pyclass(module = "aperio.text_rendering")]
pub struct PyCharGlyphData {
    pub inner: CharGlyphData,
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

    /// テキストをシェイプしてグリフをアトラスに登録し、描画準備済みデータを返す。
    /// グリフが存在しない場合（空文字列・スペースのみ等）は `None` を返す。
    pub fn prepare_render_text(
        &mut self,
        spec: &PyTextSpec,
    ) -> PyResult<Option<PyPreparedText>> {
        let prepared = self.inner.prepare_render_text(&spec.inner)?;
        Ok(prepared.map(|p| PyPreparedText { inner: p }))
    }

    /// `PyPreparedText` を受け取り GPU レンダーパスを実行して出力テクスチャを返す。
    pub fn run_render_text(&mut self, prepared: &PyPreparedText) -> PyResult<PyTexture> {
        let tex = self.inner.run_render_text(&prepared.inner)?;
        let width = tex.width();
        let height = tex.height();
        Ok(PyTexture { inner: tex, width, height })
    }

    /// `PyPreparedText` を受け取り、文字ごとに切り抜いたテクスチャのリストを返す。
    /// 戻り値は `(PyCharGlyphData, PyTexture)` のリスト（文字順）。
    pub fn run_render_chars(
        &mut self,
        prepared: &PyPreparedText,
    ) -> PyResult<Vec<(PyCharGlyphData, PyTexture)>> {
        let results = self.inner.run_render_chars(&prepared.inner)?;
        Ok(results
            .into_iter()
            .map(|(glyph_data, tex)| {
                let width = tex.width();
                let height = tex.height();
                (
                    PyCharGlyphData { inner: glyph_data },
                    PyTexture { inner: tex, width, height },
                )
            })
            .collect())
    }

    /// パイプライン経由でテキストをレンダリングする利便メソッド。
    /// `prepare_render_text` + `run_render_text` を一括で実行する。
    pub fn render_text_for_pipeline(
        &mut self,
        _inputs: Vec<Py<PyTexture>>,
        prepared: &PyPreparedText,
    ) -> PyResult<Option<PyTexture>> {
        let tex = self.inner.run_render_text(&prepared.inner)?;
        let width = tex.width();
        let height = tex.height();
        Ok(Some(PyTexture { inner: tex, width, height }))
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyPreparedText {
    /// 出力テクスチャの幅（px）
    #[getter]
    pub fn width(&self) -> u32 {
        self.inner.width
    }

    /// 出力テクスチャの高さ（px）
    #[getter]
    pub fn height(&self) -> u32 {
        self.inner.height
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyCharGlyphData {
    /// 文字
    #[getter]
    pub fn ch(&self) -> char {
        self.inner.ch
    }

    /// 全体テクスチャ上の左端 x 座標（px）
    #[getter]
    pub fn x(&self) -> u32 {
        self.inner.x
    }

    /// 全体テクスチャ上の上端 y 座標（px）
    #[getter]
    pub fn y(&self) -> u32 {
        self.inner.y
    }

    /// 文字グリフの幅（px）
    #[getter]
    pub fn w(&self) -> u32 {
        self.inner.w
    }

    /// 文字グリフの高さ（px）
    #[getter]
    pub fn h(&self) -> u32 {
        self.inner.h
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
                font_family,
                max_width,
                line_spacing,
                char_spacing,
            },
        }
    }
}

#[pymodule]
pub fn text_rendering_register(m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<PyTextRenderer>()?;
    m.add_class::<PyTextSpec>()?;
    m.add_class::<PyPreparedText>()?;
    m.add_class::<PyCharGlyphData>()?;
    Ok(())
}

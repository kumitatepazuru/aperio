use avloader::{ColorFormat, VideoLoader};
use numpy::{ndarray::Array3, IntoPyArray, PyArray3};
use pyo3::{exceptions::PyValueError, prelude::*};

use crate::python::modules::gpu_util::*;

// ─────────────────────────────────────────────────────────────
//  ColorFormat ラッパー
// ─────────────────────────────────────────────────────────────

#[pyclass(from_py_object, name = "ColorFormat", eq)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PyColorFormat {
    RgbUnorm,
    RgbaUnorm,
    Rgb16Float,
    Rgba16Float,
}

impl From<ColorFormat> for PyColorFormat {
    fn from(cf: ColorFormat) -> Self {
        match cf {
            ColorFormat::RgbUnorm => Self::RgbUnorm,
            ColorFormat::RgbaUnorm => Self::RgbaUnorm,
            ColorFormat::Rgb16Float => Self::Rgb16Float,
            ColorFormat::Rgba16Float => Self::Rgba16Float,
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  VideoLoader ラッパー
// ─────────────────────────────────────────────────────────────

/// 1 つの動画ファイルを扱うローダー。
///
/// ```python
/// loader = VideoLoader("/path/to/video.mp4", 30.0, image_generator)
///
/// # shape: (height, width, channels) dtype: uint8
/// frame = loader.get_frame(1)
///
/// # GPU テクスチャ (Rgba8Unorm)
/// tex = loader.get_texture_frame(1)
/// ```

#[pyclass]
pub struct PyVideoLoader {
    inner: VideoLoader,
}

#[pymethods]
impl PyVideoLoader {
    /// 動画ファイルを開く。
    ///
    /// # Arguments
    /// - `path`            : 動画ファイルパス（UTF-8）
    /// - `image_generator` : GPU リソース管理（`PyImageGenerator`）
    #[new]
    pub fn new(path: &str, image_generator: &PyImageGenerator) -> PyResult<Self> {
        let inner = VideoLoader::new(path, image_generator.inner.clone())
            .map_err(|e| PyValueError::new_err(format!("VideoLoader::new: {e}")))?;
        Ok(Self { inner })
    }

    /// 動画の幅（px）。
    #[getter]
    pub fn width(&self) -> u32 {
        self.inner.get_width()
    }

    /// 動画の高さ（px）。
    #[getter]
    pub fn height(&self) -> u32 {
        self.inner.get_height()
    }

    /// カラーフォーマット。
    #[getter]
    pub fn color_format(&self) -> PyColorFormat {
        self.inner.get_color_format().into()
    }

    /// 動画の長さ（秒）。
    #[getter]
    pub fn duration(&self) -> f64 {
        self.inner.get_duration()
    }

    /// 指定フレーム（1 始まり）を numpy 配列で返す。
    ///
    /// **shape**: `(height, width, channels)` — channels は 3 (RGB) または 4 (RGBA)
    /// **dtype**: `uint8`
    ///
    /// GStreamer バッファ → Vec への 1 回コピー後、Vec の所有権を numpy に移譲するため
    /// 追加のメモリコピーは発生しない。
    pub fn get_frame<'py>(
        &self,
        py: Python<'py>,
        frame_number: u64,
        target_fps: f64,
    ) -> PyResult<Bound<'py, PyArray3<u8>>> {
        let data = self
            .inner
            .get_frame(frame_number, target_fps)
            .map_err(|e| PyValueError::new_err(format!("get_frame: {e}")))?;

        let h = self.inner.get_height() as usize;
        let w = self.inner.get_width() as usize;
        let c = self.inner.get_color_format().bytes_per_pixel();

        // Vec → ndarray::Array3 → numpy（所有権移譲、コピーなし）
        let arr = Array3::<u8>::from_shape_vec((h, w, c), data)
            .map_err(|e| PyValueError::new_err(format!("reshape failed: {e}")))?;
        Ok(arr.into_pyarray(py))
    }

    /// 指定フレームを GPU テクスチャ（`PyTexture`, Rgba8Unorm）で返す。
    ///
    /// ネイティブ YUV フォーマット（I420/NV12/I422/I444 など）のまま GPU に転送するため
    /// CPU→GPU のデータ量を削減し、chroma subsampling の品質劣化が起きない。
    pub fn get_texture_frame(&self, frame_number: u64, target_fps: f64) -> PyResult<PyTexture> {
        let tex = self.inner.get_texture_frame(frame_number, target_fps)?;
        let width = tex.width();
        let height = tex.height();
        Ok(PyTexture {
            inner: tex,
            width,
            height,
        })
    }

    /// パイプライン経由で指定フレームを GPU テクスチャで返す利便メソッド。
    /// params は `(frame_number, target_fps)` のタプル。
    pub fn get_frame_for_pipeline(
        &mut self,
        _inputs: Vec<Py<PyTexture>>,
        params: (u64, f64),
    ) -> PyResult<Option<PyTexture>> {
        let (frame_number, target_fps) = params;
        self.get_texture_frame(frame_number, target_fps).map(Some)
    }
}

// ─────────────────────────────────────────────────────────────
//  モジュール登録
// ─────────────────────────────────────────────────────────────

#[pymodule(name = "avloader")]
pub mod avloader_register {
    #[pymodule_export]
    use super::PyColorFormat;
    #[pymodule_export]
    use super::PyVideoLoader;
}

use avloader::{AudioLoader, ColorFormat, VideoLoader};
use numpy::{ndarray::Array2, ndarray::Array3, IntoPyArray, PyArray2, PyArray3};
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

    /// 動画のネイティブフレームレート（fps）。
    #[getter]
    pub fn fps(&self) -> f64 {
        self.inner.get_fps()
    }

    /// 動画の正確なフレーム数。
    #[getter]
    pub fn frame_count(&self) -> i64 {
        self.inner.get_frame_count()
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
//  AudioLoader ラッパー
// ─────────────────────────────────────────────────────────────

/// 1 つのオーディオファイル（または動画ファイルの音声トラック）を扱うローダー。
///
/// ```python
/// loader = AudioLoader("/path/to/audio.wav")
///
/// # shape: (channels, samples)  dtype: float32
/// # e.g. stereo → [[L0, L1, …], [R0, R1, …]]
/// waveform = loader.get_audio(time=0.0, duration=1.0)
/// ```

#[pyclass]
pub struct PyAudioLoader {
    inner: AudioLoader,
}

#[pymethods]
impl PyAudioLoader {
    /// オーディオファイルを開く。
    ///
    /// # Arguments
    /// - `path` : ファイルパス（UTF-8）。動画ファイルを渡すと音声トラックを開く。
    #[new]
    pub fn new(path: &str) -> PyResult<Self> {
        let inner = AudioLoader::new(path)
            .map_err(|e| PyValueError::new_err(format!("AudioLoader::new: {e}")))?;
        Ok(Self { inner })
    }

    /// チャンネル数。
    #[getter]
    pub fn chs(&self) -> u32 {
        self.inner.get_chs()
    }

    /// 最大再生時間（秒）。
    #[getter]
    pub fn duration(&self) -> f64 {
        self.inner.get_duration()
    }

    /// ビットレート（bps）。
    #[getter]
    pub fn bitrate(&self) -> i64 {
        self.inner.get_bitrate()
    }

    /// サンプリングレート（Hz）。
    #[getter]
    pub fn sampling_rate(&self) -> u32 {
        self.inner.get_sampling_rate()
    }

    /// `time` 秒から `duration` 秒分の波形データを numpy 配列で返す。
    ///
    /// **shape**: `(channels, samples)`  **dtype**: `float32`
    ///
    /// ステレオ例: `[[L0, L1, …], [R0, R1, …]]`
    pub fn get_audio<'py>(
        &self,
        py: Python<'py>,
        time: f64,
        duration: f64,
    ) -> PyResult<Bound<'py, PyArray2<f32>>> {
        let channels = self
            .inner
            .get_audio(time, duration)
            .map_err(|e| PyValueError::new_err(format!("get_audio: {e}")))?;

        let n_chs = channels.len();
        let n_samples = channels.first().map_or(0, |c| c.len());

        // Vec<Vec<f32>> → 平坦化して Array2 に変換（コピー 1 回）
        let flat: Vec<f32> = channels.into_iter().flatten().collect();
        let arr = Array2::<f32>::from_shape_vec((n_chs, n_samples), flat)
            .map_err(|e| PyValueError::new_err(format!("reshape failed: {e}")))?;
        Ok(arr.into_pyarray(py))
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
    #[pymodule_export]
    use super::PyAudioLoader;
}

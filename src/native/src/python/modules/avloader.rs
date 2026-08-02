use avloader::{AudioLoader, ColorFormat, ImageLoader, VideoLoader};
use half::f16;
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

/// リトルエンディアン f16 のタイトパック済みバイト列を `(height, width, channels)`
/// の numpy 配列（dtype `float16`）に変換する。VideoLoader/ImageLoader 共通。
fn f16_bytes_to_pyarray<'py>(
    py: Python<'py>,
    bytes: &[u8],
    height: usize,
    width: usize,
    channels: usize,
) -> PyResult<Bound<'py, PyArray3<f16>>> {
    let half_data: Vec<f16> = bytes
        .chunks_exact(2)
        .map(|b| f16::from_le_bytes([b[0], b[1]]))
        .collect();
    let arr = Array3::<f16>::from_shape_vec((height, width, channels), half_data)
        .map_err(|e| PyValueError::new_err(format!("reshape failed: {e}")))?;
    Ok(arr.into_pyarray(py))
}

// ─────────────────────────────────────────────────────────────
//  VideoLoader ラッパー
// ─────────────────────────────────────────────────────────────

/// 1 つの動画ファイルを扱うローダー。
///
/// ```python
/// loader = VideoLoader("/path/to/video.mp4", 30.0, image_generator)
///
/// # shape: (height, width, channels) dtype: float16
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
    /// **dtype**: `float16` — ソースのビット深度によらず常に f16 で返す
    /// （YUV 平面から CPU 上で直接変換するため、10/12/16bit ソースも精度を落とさない）。
    pub fn get_frame<'py>(
        &self,
        py: Python<'py>,
        frame_number: u64,
        target_fps: f64,
    ) -> PyResult<Bound<'py, PyArray3<f16>>> {
        let data = self
            .inner
            .get_frame(frame_number, target_fps)
            .map_err(|e| PyValueError::new_err(format!("get_frame: {e}")))?;

        let h = self.inner.get_height() as usize;
        let w = self.inner.get_width() as usize;
        let c = self.inner.get_color_format().channel_count();

        f16_bytes_to_pyarray(py, &data, h, w, c)
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
//  ImageLoader ラッパー
// ─────────────────────────────────────────────────────────────

/// 1 つの静止画ファイルを扱うローダー。VideoLoader と同じ `avloader_video_*` FFI を
/// 流用して開く（FFmpeg は静止画も1フレームの動画として開けるため）。動画ファイルを
/// 渡した場合も検証・拒否はせず、最初の1フレームのみをデコードする。
///
/// ```python
/// loader = ImageLoader("/path/to/image.png", image_generator)
///
/// # shape: (height, width, channels) dtype: float16
/// frame = loader.get_frame()
///
/// # GPU テクスチャ (Rgba16Float)
/// tex = loader.get_texture_frame()
/// ```
#[pyclass]
pub struct PyImageLoader {
    inner: ImageLoader,
}

#[pymethods]
impl PyImageLoader {
    /// 画像ファイルを開く。デコード（YUV 平面・GPU テクスチャの両方）はコンストラクタ
    /// 内で一度だけ実行される。`get_frame()` の RGB(A) バイト列への変換は初回呼び出し
    /// まで遅延され、以降はキャッシュを返す。
    ///
    /// # Arguments
    /// - `path`            : 画像ファイルパス（UTF-8）
    /// - `image_generator` : GPU リソース管理（`PyImageGenerator`）
    #[new]
    pub fn new(path: &str, image_generator: &PyImageGenerator) -> PyResult<Self> {
        let inner = ImageLoader::new(path, image_generator.inner.clone())
            .map_err(|e| PyValueError::new_err(format!("ImageLoader::new: {e}")))?;
        Ok(Self { inner })
    }

    /// 画像の幅（px）。
    #[getter]
    pub fn width(&self) -> u32 {
        self.inner.get_width()
    }

    /// 画像の高さ（px）。
    #[getter]
    pub fn height(&self) -> u32 {
        self.inner.get_height()
    }

    /// カラーフォーマット。
    #[getter]
    pub fn color_format(&self) -> PyColorFormat {
        self.inner.get_color_format().into()
    }

    /// 画像を numpy 配列で返す。
    ///
    /// **shape**: `(height, width, channels)` — channels は 3 (RGB) または 4 (RGBA)
    /// **dtype**: `float16`
    ///
    /// 初回呼び出し時にのみ YUV 平面から CPU で変換し、以降はキャッシュを返す。
    pub fn get_frame<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyArray3<f16>>> {
        let data = self
            .inner
            .get_frame()
            .map_err(|e| PyValueError::new_err(format!("get_frame: {e}")))?;
        let h = self.inner.get_height() as usize;
        let w = self.inner.get_width() as usize;
        let c = self.inner.get_color_format().channel_count();

        f16_bytes_to_pyarray(py, &data, h, w, c)
    }

    /// 画像を GPU テクスチャ（`PyTexture`, Rgba16Float）で返す。
    ///
    /// コンストラクタで構築済みのテクスチャを共有する（Arc 参照カウント増加のみ、
    /// 追加のデコード・GPU 転送は発生しない）。
    pub fn get_texture_frame(&self) -> PyTexture {
        let tex = self.inner.get_texture_frame();
        let width = tex.width();
        let height = tex.height();
        PyTexture {
            inner: tex,
            width,
            height,
        }
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

    /// ビット深度（bits per sample）。
    #[getter]
    pub fn bit_depth(&self) -> i32 {
        self.inner.get_bit_depth()
    }

    /// サンプリングレート（Hz）。
    #[getter]
    pub fn sampling_rate(&self) -> u32 {
        self.inner.get_sampling_rate()
    }

    /// `time_samples` から `duration_samples` サンプル分の波形データを numpy 配列で返す。
    ///
    /// `time_samples` / `duration_samples` は **出力（target）`sample_rate` 単位**の
    /// サンプル数で指定する（秒 × sample_rate で変換）。
    ///
    /// **shape**: `(channels, samples)`  **dtype**: `float32`
    ///
    /// `sample_rate` と `channels` を指定した場合は swresample で一括変換する。
    /// 省略するとネイティブのサンプルレート・チャンネル数のまま返す。
    ///
    /// ステレオ例: `[[L0, L1, …], [R0, R1, …]]`
    #[pyo3(signature = (time_samples, duration_samples, *, sample_rate=None, channels=None))]
    pub fn get_audio<'py>(
        &self,
        py: Python<'py>,
        time_samples: i64,
        duration_samples: i64,
        sample_rate: Option<u32>,
        channels: Option<u32>,
    ) -> PyResult<Bound<'py, PyArray2<f32>>> {
        let sr = sample_rate.unwrap_or_else(|| self.inner.get_sampling_rate());
        let chs = channels.unwrap_or_else(|| self.inner.get_chs());
        let raw = self
            .inner
            .get_audio(time_samples, duration_samples, sr, chs)
            .map_err(|e| PyValueError::new_err(format!("get_audio: {e}")))?;

        let n_chs = raw.len();
        let n_samples = raw.first().map_or(0, |c| c.len());

        // Vec<Vec<f32>> → 平坦化して Array2 に変換（コピー 1 回）
        let flat: Vec<f32> = raw.into_iter().flatten().collect();
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
    use super::PyAudioLoader;
    #[pymodule_export]
    use super::PyColorFormat;
    #[pymodule_export]
    use super::PyImageLoader;
    #[pymodule_export]
    use super::PyVideoLoader;
}

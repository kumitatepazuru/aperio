use crate::common::{self, ColorFormat};
use crate::ffi::*;
use crate::frame_cache::{DecoderRef, FrameCache};
use crate::yuv_pipeline::{PlaneDesc, YuvPipeline};
use anyhow::{bail, Context, Result};
use gpu_util::image_generator::ImageGenerator;
use std::sync::Arc;

// ─── VideoLoader ─────────────────────────────────────────────────────────────

pub struct VideoLoader {
    // frame_cache must be declared before decoder so it drops first,
    // joining the background thread before avloader_video_close is called.
    frame_cache: FrameCache,
    decoder: Arc<DecoderRef>, // shared with FrameCache's bg thread

    width: u32,
    height: u32,
    color_format: ColorFormat,
    fps: f64,

    plane_descs: Vec<PlaneDesc>,
    image_generator: ImageGenerator,
    yuv_pipeline: YuvPipeline,
}

// SAFETY: All C++ decoder calls are serialised by an internal std::mutex.
// Python GIL additionally ensures single-threaded access at the call-site level.
unsafe impl Send for VideoLoader {}
unsafe impl Sync for VideoLoader {}

impl VideoLoader {
    pub fn new(path: &str, image_generator: ImageGenerator) -> Result<Self> {
        let c_path = std::ffi::CString::new(path).context("Video path contains null byte")?;

        let handle = unsafe { avloader_video_open(c_path.as_ptr()) };
        if handle.is_null() {
            bail!("avloader_video_open failed: could not open \"{}\"", path);
        }

        let probed = common::probe_media(handle);
        let fps = unsafe { avloader_video_native_fps(handle) };

        let yuv_pipeline = YuvPipeline::new(&image_generator.device, probed.layout, probed.yuv_params);
        let decoder = Arc::new(DecoderRef(handle));
        let frame_cache = FrameCache::new(Arc::clone(&decoder), probed.plane_descs.clone(), fps);

        Ok(Self {
            frame_cache,
            decoder,
            width: probed.width,
            height: probed.height,
            color_format: probed.color_format,
            fps,
            plane_descs: probed.plane_descs,
            image_generator,
            yuv_pipeline,
        })
    }

    pub fn get_width(&self) -> u32 {
        self.width
    }
    pub fn get_height(&self) -> u32 {
        self.height
    }
    pub fn get_color_format(&self) -> ColorFormat {
        self.color_format
    }
    pub fn get_fps(&self) -> f64 {
        self.fps
    }
    pub fn get_frame_count(&self) -> i64 {
        unsafe { crate::ffi::avloader_video_frame_count(self.decoder.0) }
    }

    // ── get_frame ──────────────────────────────────────────────────────────
    /// Decode frame `frame_number` (1-based) and return tightly packed
    /// little-endian `half::f16` RGB/RGBA bytes. Uses the C++ sws_scale path
    /// (16-bit RGB48LE/RGBA64LE, normalized to f16 in Rust); does not go through
    /// the prefetch cache. Always f16 regardless of source bit depth, so
    /// 10/12/16-bit sources keep their full precision.
    pub fn get_frame(&self, frame_number: u64, target_fps: f64) -> Result<Vec<u8>> {
        common::decode_rgb_f16(
            self.decoder.0,
            frame_number,
            target_fps,
            self.width,
            self.height,
            self.color_format.channel_count() as i32,
        )
    }

    // ── get_texture_frame ──────────────────────────────────────────────────
    /// Return frame `frame_number` as a GPU Rgba16Float texture.
    ///
    /// Frame data is served from the prefetch cache when available (sequential
    /// playback), from the LRU cache on repeated random seeks, or decoded on
    /// demand on first access. The YUV→RGBA conversion runs on the GPU.
    pub fn get_texture_frame(
        &self,
        frame_number: u64,
        target_fps: f64,
    ) -> Result<Arc<wgpu::Texture>> {
        let cached = self
            .frame_cache
            .get(frame_number, target_fps)
            .ok_or_else(|| anyhow::anyhow!("Failed to decode frame {}", frame_number))?;

        let plane_slices: Vec<&[u8]> = cached.planes.iter().map(|v| v.as_slice()).collect();

        self.yuv_pipeline.convert(
            &self.image_generator,
            &self.plane_descs,
            &plane_slices,
            self.width,
            self.height,
        )
    }
}
// VideoLoader::drop is implicit: frame_cache drops first (joins bg thread),
// then decoder drops (last Arc → avloader_video_close).

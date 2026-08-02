use crate::common::{self, ColorFormat};
use crate::ffi::*;
use crate::frame_cache::{self, DecoderRef};
use crate::yuv_pipeline::YuvPipeline;
use anyhow::{bail, Context, Result};
use gpu_util::image_generator::ImageGenerator;
use std::sync::{Arc, Mutex};

// ─── ImageLoader ─────────────────────────────────────────────────────────────

/// Loads a single still image via the same `avloader_video_*` FFI as `VideoLoader`
/// (FFmpeg opens a still image as a 1-frame video stream). If `path` points at an
/// actual multi-frame video, only the first frame is decoded and the rest is
/// ignored — this is not validated/rejected.
pub struct ImageLoader {
    decoder: DecoderRef, // kept alive for the lifetime of the loader, like VideoLoader
    width: u32,
    height: u32,
    color_format: ColorFormat,
    safe_fps: f64, // used internally by the lazy get_frame() decode call
    texture_frame: Arc<wgpu::Texture>,
    rgb_cache: Mutex<Option<Arc<Vec<u8>>>>,
}

// No unsafe impl Send/Sync needed: every field (DecoderRef, Arc<wgpu::Texture>,
// Mutex<..>, primitives) is already Send + Sync, so it's auto-derived. DecoderRef
// itself carries the SAFETY rationale (C++ decoder calls are serialised by an
// internal std::mutex) in frame_cache.rs.

impl ImageLoader {
    pub fn new(path: &str, image_generator: ImageGenerator) -> Result<Self> {
        let c_path = std::ffi::CString::new(path).context("Image path contains null byte")?;

        let handle = unsafe { avloader_video_open(c_path.as_ptr()) };
        if handle.is_null() {
            bail!("avloader_video_open failed: could not open \"{}\"", path);
        }
        let decoder = DecoderRef(handle);

        let probed = common::probe_media(decoder.0);

        let native_fps = unsafe { avloader_video_native_fps(decoder.0) };
        // frame_num is hardcoded to 1, so target_time's numerator is always 0.0;
        // but target_fps=0 would still turn that into 0.0/0.0 = NaN in the C++
        // decode_to_time(), poisoning every comparison there. Guard with a
        // fallback that's never exposed on ImageLoader's public API.
        let safe_fps = if native_fps > 0.0 { native_fps } else { 1.0 };

        let cached = frame_cache::decode_one(&decoder, 1, &probed.plane_descs, safe_fps)
            .ok_or_else(|| anyhow::anyhow!("avloader_video_decode_frame failed for \"{}\"", path))?;

        let yuv_pipeline = YuvPipeline::new(&image_generator.device, probed.layout, probed.yuv_params);
        let plane_slices: Vec<&[u8]> = cached.planes.iter().map(|v| v.as_slice()).collect();
        let texture_frame = yuv_pipeline.convert(
            &image_generator,
            &probed.plane_descs,
            &plane_slices,
            probed.width,
            probed.height,
        )?;

        Ok(Self {
            decoder,
            width: probed.width,
            height: probed.height,
            color_format: probed.color_format,
            safe_fps,
            texture_frame,
            rgb_cache: Mutex::new(None),
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

    /// Returns the GPU Rgba16Float texture built once at construction time.
    /// Cheap `Arc` clone — no re-decode or re-conversion.
    pub fn get_texture_frame(&self) -> Arc<wgpu::Texture> {
        Arc::clone(&self.texture_frame)
    }

    /// Returns tightly packed little-endian `half::f16` RGB/RGBA bytes, decoded
    /// via the C++ sws_scale path (16-bit RGB48LE/RGBA64LE, normalized to f16 in
    /// Rust). Decoded lazily on first call and cached thereafter.
    pub fn get_frame(&self) -> Result<Arc<Vec<u8>>> {
        let mut guard = self.rgb_cache.lock().unwrap();
        if let Some(cached) = &*guard {
            return Ok(Arc::clone(cached));
        }
        let bytes = common::decode_rgb_f16(
            self.decoder.0,
            1,
            self.safe_fps,
            self.width,
            self.height,
            self.color_format.channel_count() as i32,
        )?;
        let arc = Arc::new(bytes);
        *guard = Some(Arc::clone(&arc));
        Ok(arc)
    }
}

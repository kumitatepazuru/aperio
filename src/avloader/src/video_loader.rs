use crate::ffi::*;
use crate::yuv_pipeline::{PlaneDesc, YuvLayout, YuvPipeline};
use anyhow::{bail, Context, Result};
use gpu_util::image_generator::ImageGenerator;
use std::sync::Arc;

// ─── ColorFormat ─────────────────────────────────────────────────────────────

/// Output color format for CPU-side frames (get_frame).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorFormat {
    RgbUnorm,
    RgbaUnorm,
    Rgb16Float,
    Rgba16Float,
}

impl ColorFormat {
    /// Bytes per pixel in the output Vec<u8>.
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            ColorFormat::RgbUnorm   => 3,
            ColorFormat::RgbaUnorm  => 4,
            ColorFormat::Rgb16Float  => 6,
            ColorFormat::Rgba16Float => 8,
        }
    }
    fn channels(self) -> i32 {
        match self {
            ColorFormat::RgbUnorm | ColorFormat::Rgb16Float   => 3,
            ColorFormat::RgbaUnorm | ColorFormat::Rgba16Float => 4,
        }
    }
}

// ─── AVPixelFormat constants (subset, matches FFmpeg) ─────────────────────────
// We keep only what we need to avoid pulling in FFmpeg headers into Rust.
const AV_PIX_FMT_YUVA420P: i32 = 33;
const AV_PIX_FMT_NV12:     i32 = 23;
const AV_PIX_FMT_NV21:     i32 = 24;
const AV_PIX_FMT_NV16:     i32 = 82;

fn color_format_from_pix_fmt(pix_fmt: i32, _has_alpha: bool) -> ColorFormat {
    // High-bit-depth → 16float; alpha channel → RGBA; else → RGB
    let has_alpha = matches!(pix_fmt, AV_PIX_FMT_YUVA420P | 74 | 75 | 76); // yuva420p/422p/444p
    if has_alpha { ColorFormat::RgbaUnorm } else { ColorFormat::RgbUnorm }
}

fn yuv_layout_from_pix_fmt(pix_fmt: i32) -> YuvLayout {
    match pix_fmt {
        f if f == AV_PIX_FMT_NV12 || f == AV_PIX_FMT_NV21 || f == AV_PIX_FMT_NV16 => {
            YuvLayout::SemiPlanar
        }
        _ => YuvLayout::Planar,
    }
}

// ─── VideoLoader ─────────────────────────────────────────────────────────────

pub struct VideoLoader {
    handle: AvLoaderHandle,

    // Properties cached on the Rust side (no C++ round-trips needed)
    width:        u32,
    height:       u32,
    color_format: ColorFormat,
    fps:          f64,

    // Per-plane descriptors (for get_texture_frame)
    plane_descs: Vec<PlaneDesc>,

    image_generator: ImageGenerator,
    yuv_pipeline:    YuvPipeline,
}

// Safety: AvLoaderHandle is an opaque pointer. We ensure single-threaded access
// via Python's GIL (the only caller is PyVideoLoader under the GIL).
unsafe impl Send for VideoLoader {}
unsafe impl Sync for VideoLoader {}

impl VideoLoader {
    pub fn new(path: &str, target_fps: f64, image_generator: ImageGenerator) -> Result<Self> {
        let c_path = std::ffi::CString::new(path)
            .context("Video path contains null byte")?;

        let handle = unsafe { avloader_open(c_path.as_ptr(), target_fps) };
        if handle.is_null() {
            bail!("avloader_open failed: could not open \"{}\"", path);
        }

        let width    = unsafe { avloader_width(handle) }  as u32;
        let height   = unsafe { avloader_height(handle) } as u32;
        let pix_fmt  = unsafe { avloader_pixel_format(handle) };
        let fps      = unsafe { avloader_native_fps(handle) };

        let color_format = color_format_from_pix_fmt(pix_fmt, false);
        let layout       = yuv_layout_from_pix_fmt(pix_fmt);

        // Build plane descriptors
        let plane_count = unsafe { avloader_yuv_plane_count(handle) } as usize;
        let plane_descs: Vec<PlaneDesc> = (0..plane_count)
            .map(|i| {
                let mut tw = 0i32;
                let mut th = 0i32;
                let mut bpt = 0i32;
                unsafe { avloader_yuv_plane_info(handle, i as i32, &mut tw, &mut th, &mut bpt) };
                let bpt = bpt.max(1) as u32;
                let format = if bpt == 2 {
                    wgpu::TextureFormat::Rg8Unorm
                } else {
                    wgpu::TextureFormat::R8Unorm
                };
                PlaneDesc {
                    tex_width:       tw.max(1) as u32,
                    tex_height:      th.max(1) as u32,
                    bytes_per_texel: bpt,
                    format,
                }
            })
            .collect();

        let yuv_pipeline = YuvPipeline::new(&image_generator.device, layout);

        Ok(Self {
            handle,
            width,
            height,
            color_format,
            fps,
            plane_descs,
            image_generator,
            yuv_pipeline,
        })
    }

    // ── property accessors ─────────────────────────────────────────────────
    pub fn get_width(&self)        -> u32         { self.width }
    pub fn get_height(&self)       -> u32         { self.height }
    pub fn get_color_format(&self) -> ColorFormat { self.color_format }
    pub fn get_fps(&self)          -> f64         { self.fps }

    // ── get_frame ──────────────────────────────────────────────────────────
    /// Decode frame `frame_number` (1-based, relative to target_fps) and return
    /// the pixel data as [R,G,B,…] bytes.  No internal clone occurs.
    pub fn get_frame(&self, frame_number: u64) -> Result<Vec<u8>> {
        let channels = self.color_format.channels();
        let size     = (self.width as usize) * (self.height as usize)
            * self.color_format.bytes_per_pixel();

        let mut buf = vec![0u8; size];
        let ret = unsafe {
            avloader_get_frame_rgb(
                self.handle,
                frame_number,
                buf.as_mut_ptr(),
                buf.len(),
                channels,
            )
        };

        if ret < 0 {
            bail!("avloader_get_frame_rgb failed for frame {}", frame_number);
        }
        Ok(buf)
    }

    // ── get_texture_frame ──────────────────────────────────────────────────
    /// Decode frame `frame_number` in native YUV format, upload each plane to
    /// GPU, run the YUV→Rgba16Float shader, and return the output texture.
    pub fn get_texture_frame(&self, frame_number: u64) -> Result<Arc<wgpu::Texture>> {
        let num_planes = self.plane_descs.len();
        if num_planes == 0 {
            bail!("No YUV planes available for this pixel format");
        }

        // Allocate one Vec per plane
        let mut plane_data: Vec<Vec<u8>> = self
            .plane_descs
            .iter()
            .map(|d| vec![0u8; d.total_bytes()])
            .collect();

        // Collect C-compatible pointer + stride arrays
        let mut ptrs: Vec<*mut u8>  = plane_data.iter_mut().map(|v| v.as_mut_ptr()).collect();
        let strides: Vec<i32>       = self.plane_descs.iter()
            .map(|d| d.bytes_per_row() as i32)
            .collect();

        let ret = unsafe {
            avloader_get_frame_yuv(
                self.handle,
                frame_number,
                num_planes as i32,
                ptrs.as_mut_ptr(),
                strides.as_ptr(),
            )
        };

        if ret < 0 {
            bail!("avloader_get_frame_yuv failed for frame {}", frame_number);
        }

        self.yuv_pipeline.convert(
            &self.image_generator,
            &self.plane_descs,
            &plane_data,
            self.width,
            self.height,
        )
    }
}

impl Drop for VideoLoader {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { avloader_close(self.handle) };
        }
    }
}

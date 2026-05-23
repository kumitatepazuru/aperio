use crate::ffi::*;
use crate::frame_cache::{DecoderRef, FrameCache};
use crate::yuv_pipeline::{PlaneDesc, YuvConvParams, YuvLayout, YuvPipeline};
use anyhow::{bail, Context, Result};
use gpu_util::image_generator::ImageGenerator;
use std::sync::Arc;

// ─── ColorFormat ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorFormat {
    RgbUnorm,
    RgbaUnorm,
    Rgb16Float,
    Rgba16Float,
}

impl ColorFormat {
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            ColorFormat::RgbUnorm => 3,
            ColorFormat::RgbaUnorm => 4,
            ColorFormat::Rgb16Float => 6,
            ColorFormat::Rgba16Float => 8,
        }
    }
    fn channels(self) -> i32 {
        match self {
            ColorFormat::RgbUnorm | ColorFormat::Rgb16Float => 3,
            ColorFormat::RgbaUnorm | ColorFormat::Rgba16Float => 4,
        }
    }
}

// ─── AVPixelFormat constants (subset) ────────────────────────────────────────
const AV_PIX_FMT_YUVA420P: i32 = 33;
const AV_PIX_FMT_NV12: i32 = 23;
const AV_PIX_FMT_NV21: i32 = 24;
const AV_PIX_FMT_NV16: i32 = 82;
// Semi-planar 10/16-bit variants (NV12-like layout, wider samples)
const AV_PIX_FMT_P010LE: i32 = 161;
const AV_PIX_FMT_P010BE: i32 = 162;
const AV_PIX_FMT_P016LE: i32 = 166;
const AV_PIX_FMT_P016BE: i32 = 167;

fn color_format_from_pix_fmt(pix_fmt: i32) -> ColorFormat {
    let has_alpha = matches!(pix_fmt, AV_PIX_FMT_YUVA420P | 74 | 75 | 76);
    if has_alpha {
        ColorFormat::RgbaUnorm
    } else {
        ColorFormat::RgbUnorm
    }
}

fn yuv_layout_from_pix_fmt(pix_fmt: i32) -> YuvLayout {
    match pix_fmt {
        f if f == AV_PIX_FMT_NV21 => YuvLayout::SemiPlanarVU,
        f if matches!(
            f,
            AV_PIX_FMT_NV12
                | AV_PIX_FMT_NV16
                | AV_PIX_FMT_P010LE
                | AV_PIX_FMT_P010BE
                | AV_PIX_FMT_P016LE
                | AV_PIX_FMT_P016BE
        ) =>
        {
            YuvLayout::SemiPlanar
        }
        f if matches!(f, AV_PIX_FMT_YUVA420P | 74 | 75 | 76) => YuvLayout::PlanarAlpha,
        _ => YuvLayout::Planar,
    }
}

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

        let width = unsafe { avloader_video_width(handle) } as u32;
        let height = unsafe { avloader_video_height(handle) } as u32;
        let pix_fmt = unsafe { avloader_video_pixel_format(handle) };
        let fps = unsafe { avloader_video_native_fps(handle) };

        let color_format = color_format_from_pix_fmt(pix_fmt);
        let layout = yuv_layout_from_pix_fmt(pix_fmt);

        let plane_count = unsafe { avloader_video_yuv_plane_count(handle) } as usize;
        let plane_descs: Vec<PlaneDesc> = (0..plane_count)
            .map(|i| {
                let (mut tw, mut th, mut bpt) = (0i32, 0i32, 0i32);
                unsafe {
                    avloader_video_yuv_plane_info(handle, i as i32, &mut tw, &mut th, &mut bpt)
                };
                let bpt = bpt.max(1) as u32;
                // Plane 1 of a semi-planar format holds interleaved UV pairs:
                //   NV12 → bpt=2 (Rg8Unorm), P010 → bpt=4 (Rg16Unorm).
                // All other planes are single-component:
                //   8-bit → bpt=1 (R8Unorm), 10/16-bit → bpt=2 (R16Unorm).
                let is_uv =
                    matches!(layout, YuvLayout::SemiPlanar | YuvLayout::SemiPlanarVU) && i == 1;
                PlaneDesc {
                    tex_width: tw.max(1) as u32,
                    tex_height: th.max(1) as u32,
                    bytes_per_texel: bpt,
                    format: match (bpt, is_uv) {
                        (2, true) => wgpu::TextureFormat::Rg8Unorm,
                        (2, false) => wgpu::TextureFormat::R16Unorm,
                        (4, true) => wgpu::TextureFormat::Rg16Unorm,
                        _ => wgpu::TextureFormat::R8Unorm,
                    },
                }
            })
            .collect();

        // AVCOL_RANGE_JPEG = 2; anything else (including unspecified) is treated as limited.
        let is_full_range = unsafe { avloader_video_color_range(handle) } == 2;
        let y_bpt = plane_descs.first().map_or(1, |d| d.bytes_per_texel);
        let yuv_params = match (y_bpt, is_full_range) {
            (1, false) => YuvConvParams::limited_8bit(),
            (1, true) => YuvConvParams::full_range_8bit(),
            (_, false) => YuvConvParams::limited_10bit_in_16(),
            (_, true) => YuvConvParams::full_range_10bit_in_16(),
        };
        let yuv_pipeline = YuvPipeline::new(&image_generator.device, layout, yuv_params);
        let decoder = Arc::new(DecoderRef(handle));
        let frame_cache = FrameCache::new(Arc::clone(&decoder), plane_descs.clone(), fps);

        Ok(Self {
            frame_cache,
            decoder,
            width,
            height,
            color_format,
            fps,
            plane_descs,
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
    /// Decode frame `frame_number` (1-based) and return RGB/RGBA bytes.
    /// Uses the C++ sws_scale path; does not go through the prefetch cache.
    pub fn get_frame(&self, frame_number: u64, target_fps: f64) -> Result<Vec<u8>> {
        let channels = self.color_format.channels();
        let size = self.width as usize * self.height as usize * self.color_format.bytes_per_pixel();

        let mut buf = vec![0u8; size];
        let ret = unsafe {
            avloader_video_decode_frame_rgb(
                self.decoder.0,
                frame_number,
                target_fps,
                buf.as_mut_ptr(),
                buf.len(),
                channels,
            )
        };

        if ret < 0 {
            bail!(
                "avloader_video_decode_frame_rgb failed for frame {}",
                frame_number
            );
        }
        Ok(buf)
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

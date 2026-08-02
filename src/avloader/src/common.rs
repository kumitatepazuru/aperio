use crate::ffi::*;
use crate::yuv_pipeline::{PlaneDesc, YuvConvParams, YuvLayout};
use anyhow::{bail, Result};

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

    /// Number of channels (3 or 4). This is what `get_frame()`'s array shape
    /// actually uses — the output element type is always `half::f16` regardless
    /// of the Unorm/Float classification (see `decode_rgb_f16`).
    pub fn channel_count(self) -> usize {
        match self {
            ColorFormat::RgbUnorm | ColorFormat::Rgb16Float => 3,
            ColorFormat::RgbaUnorm | ColorFormat::Rgba16Float => 4,
        }
    }
}

/// Classifies alpha presence + source bit depth into a `ColorFormat`. This is a
/// precision *hint* — `get_frame()` always outputs f16 regardless of this
/// classification; only `channel_count()` drives the actual array shape.
fn resolve_color_format(has_alpha: bool, y_bytes_per_texel: u32) -> ColorFormat {
    match (has_alpha, y_bytes_per_texel > 1) {
        (false, false) => ColorFormat::RgbUnorm,
        (true, false) => ColorFormat::RgbaUnorm,
        (false, true) => ColorFormat::Rgb16Float,
        (true, true) => ColorFormat::Rgba16Float,
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

pub(crate) fn yuv_layout_from_pix_fmt(pix_fmt: i32) -> YuvLayout {
    match pix_fmt {
        AV_PIX_FMT_NV21 => YuvLayout::SemiPlanarVU,
        AV_PIX_FMT_NV12
        | AV_PIX_FMT_NV16
        | AV_PIX_FMT_P010LE
        | AV_PIX_FMT_P010BE
        | AV_PIX_FMT_P016LE
        | AV_PIX_FMT_P016BE => YuvLayout::SemiPlanar,
        AV_PIX_FMT_YUVA420P | 74 | 75 | 76 => YuvLayout::PlanarAlpha,
        _ => YuvLayout::Planar,
    }
}

// ─── ProbedMedia ─────────────────────────────────────────────────────────────

pub(crate) struct ProbedMedia {
    pub width: u32,
    pub height: u32,
    pub color_format: ColorFormat,
    pub layout: YuvLayout,
    pub plane_descs: Vec<PlaneDesc>,
    pub yuv_params: YuvConvParams,
}

/// Queries width/height/pixel-format/color-range/plane-layout from an already-open
/// `avloader_video_*` handle and derives everything a caller needs to build a
/// `YuvPipeline` for the GPU texture path. Does not read fps —
/// `VideoLoader` exposes fps publicly while `ImageLoader` only needs it internally
/// as a decode-call parameter, so each loader queries `avloader_video_native_fps` itself.
pub(crate) fn probe_media(handle: AvLoaderHandle) -> ProbedMedia {
    let width = unsafe { avloader_video_width(handle) } as u32;
    let height = unsafe { avloader_video_height(handle) } as u32;
    let pix_fmt = unsafe { avloader_video_pixel_format(handle) };

    let layout = yuv_layout_from_pix_fmt(pix_fmt);

    let plane_count = unsafe { avloader_video_yuv_plane_count(handle) } as usize;
    let plane_descs: Vec<PlaneDesc> = (0..plane_count)
        .map(|i| {
            let (mut tw, mut th, mut bpt) = (0i32, 0i32, 0i32);
            unsafe { avloader_video_yuv_plane_info(handle, i as i32, &mut tw, &mut th, &mut bpt) };
            let bpt = bpt.max(1) as u32;
            // Plane 1 of a semi-planar format holds interleaved UV pairs:
            //   NV12 → bpt=2 (Rg8Unorm), P010 → bpt=4 (Rg16Unorm).
            // All other planes are single-component:
            //   8-bit → bpt=1 (R8Unorm), 10/16-bit → bpt=2 (R16Unorm).
            let is_uv = matches!(layout, YuvLayout::SemiPlanar | YuvLayout::SemiPlanarVU) && i == 1;
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

    let has_alpha = layout == YuvLayout::PlanarAlpha;
    let y_bpt = plane_descs.first().map_or(1, |d| d.bytes_per_texel);
    let color_format = resolve_color_format(has_alpha, y_bpt);

    // AVCOL_RANGE_JPEG = 2; anything else (including unspecified) is treated as limited.
    let is_full_range = unsafe { avloader_video_color_range(handle) } == 2;
    let yuv_params = match (y_bpt, is_full_range) {
        (1, false) => YuvConvParams::limited_8bit(),
        (1, true) => YuvConvParams::full_range_8bit(),
        (_, false) => YuvConvParams::limited_10bit_in_16(),
        (_, true) => YuvConvParams::full_range_10bit_in_16(),
    };

    ProbedMedia {
        width,
        height,
        color_format,
        layout,
        plane_descs,
        yuv_params,
    }
}

// ─── decode_rgb_f16 ───────────────────────────────────────────────────────────

/// Calls `avloader_video_decode_frame_rgb` (16-bit RGB48LE/RGBA64LE output via
/// swscale, BT.709 + the source's actual range) and normalizes the returned
/// `u16` samples to tightly packed little-endian `half::f16` bytes
/// (`height * width * channels * 2` bytes). All YUV→RGB conversion and chroma
/// upsampling happens in C++/swscale — this is just a `u16 → f16` normalization.
pub(crate) fn decode_rgb_f16(
    handle: AvLoaderHandle,
    frame_num: u64,
    target_fps: f64,
    width: u32,
    height: u32,
    channels: i32,
) -> Result<Vec<u8>> {
    let sample_count = width as usize * height as usize * channels as usize;
    let mut raw = vec![0u8; sample_count * 2]; // u16 LE per sample
    let ret = unsafe {
        avloader_video_decode_frame_rgb(
            handle,
            frame_num,
            target_fps,
            raw.as_mut_ptr(),
            raw.len(),
            channels,
        )
    };
    if ret < 0 {
        bail!(
            "avloader_video_decode_frame_rgb failed for frame {}",
            frame_num
        );
    }

    let mut out = vec![0u8; sample_count * 2]; // f16 LE per sample
    for i in 0..sample_count {
        let u = u16::from_le_bytes([raw[i * 2], raw[i * 2 + 1]]);
        let f = half::f16::from_f32(u as f32 / 65535.0);
        out[i * 2..i * 2 + 2].copy_from_slice(&f.to_le_bytes());
    }
    Ok(out)
}

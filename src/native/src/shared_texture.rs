// gpu_utilは独立したcrateであるため、Aperio本体にはnapi-rsが変換できる型にするために再度同じものを用意する

use std::os::fd::RawFd;

use drm_fourcc::DrmFourcc;
use gpu_util::texture_to_native::OffscreenSharedTextureInfo;
use napi_derive::napi;

// https://www.electronjs.org/docs/latest/api/structures/size
#[napi(object)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

// https://www.electronjs.org/docs/latest/api/structures/color-space
#[napi(object)]
pub struct ColorSpace {
    #[napi(
        ts_type = "'bt709' | 'bt470m' | 'bt470bg' | 'smpte170m' | 'smpte240m' | 'film' | 'bt2020' | 'smptest428-1' | 'smptest431-2' | 'p3' | 'xyz-d50' | 'adobe-rgb' | 'apple-generic-rgb' | 'wide-gamut-color-spin' | 'ebu-3213-e' | 'custom' | 'invalid'"
    )]
    pub primaries: String,
    #[napi(
        ts_type = "'bt709' | 'bt709-apple' | 'gamma18' | 'gamma22' | 'gamma24' | 'gamma28' | 'smpte170m' | 'smpte240m' | 'linear' | 'log' | 'log-sqrt' | 'iec61966-2-4' | 'bt1361-ecg' | 'srgb' | 'bt2020-10' | 'bt2020-12' | 'pq' | 'smptest428-1' | 'hlg' | 'srgb-hdr' | 'linear-hdr' | 'custom' | 'custom-hdr' | 'scrgb-linear-80-nits' | 'invalid'"
    )]
    pub transfer: String,
    #[napi(
        ts_type = "'rgb' | 'bt709' | 'fcc' | 'bt470bg' | 'smpte170m' | 'smpte240m' | 'ycocg' | 'bt2020-ncl' | 'ydzdx' | 'gbr' | 'invalid'"
    )]
    pub matrix: String,
    #[napi(ts_type = "'limited' | 'full' | 'derived' | 'invalid'")]
    pub range: String,
}

// https://www.electronjs.org/docs/latest/api/structures/rectangle
#[napi(object)]
pub struct Rectangle {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

// https://chromium.googlesource.com/chromium/src/+/refs/heads/main/media/base/video_frame_metadata.h
#[napi(object)]
pub struct Metadata {
    pub capture_update_rect: Option<Rectangle>,
    pub region_capture_rect: Option<Rectangle>,
    pub source_size: Option<Rectangle>,
    pub frame_count: Option<u32>,
}

#[napi(object)]
pub struct SharedTexturePlane {
    #[napi(ts_type = "number")]
    pub fd: RawFd,
    pub stride: u32,
    pub offset: u32,
    pub size: u32,
}

#[napi(object)]
pub struct SharedTextureHandleNativePixmap {
    pub planes: Vec<SharedTexturePlane>,
    pub modifier: String,
    pub supports_zero_copy_web_gpu_import: bool,
}

// https://www.electronjs.org/docs/latest/api/structures/shared-texture-handle
#[napi(object)]
#[cfg(target_os = "linux")]
pub struct SharedTextureHandle {
    pub native_pixmap: SharedTextureHandleNativePixmap,
}

#[napi(object)]
#[cfg(target_os = "windows")]
pub struct SharedTextureHandle {
    pub nt_handle: None, // TODO
}

#[napi(object)]
#[cfg(target_os = "macos")]
pub struct SharedTextureHandle {
    pub io_surface: None, // TODO
}

// https://www.electronjs.org/docs/latest/api/structures/offscreen-shared-texture
#[napi(object)]
pub struct NodeOffscreenSharedTextureInfo {
    pub widget_type: String,
    #[napi(ts_type = "'bgra' | 'rgba' | 'rgbaf16'")]
    pub pixel_format: String,
    pub coded_size: Size,
    pub color_space: ColorSpace,
    pub visible_rect: Rectangle,
    pub content_rect: Rectangle,
    pub timestamp: u32,
    pub metadata: Option<Metadata>,
    pub handle: SharedTextureHandle,
}

impl From<&OffscreenSharedTextureInfo> for NodeOffscreenSharedTextureInfo {
    fn from(value: &OffscreenSharedTextureInfo) -> Self {
        #[cfg(target_os = "linux")]
        let handle = SharedTextureHandle {
            native_pixmap: SharedTextureHandleNativePixmap {
                planes: value
                    .handle
                    .native_pixmap
                    .planes
                    .iter()
                    .enumerate()
                    .map(|(i, plane)| {
                        SharedTexturePlane {
                            fd: plane.fd,
                            stride: 7680, // 1920 * 4
                            // stride: plane.stride,
                            offset: plane.offset,
                            size: plane.size,
                        }
                    })
                    .collect(),
                // https://github.com/electron/electron/pull/47317/files#diff-2b1bd8f20800c083271c8cd2bba6095cec7b50c97b11dd5035c21ca8ad600f73R543
                modifier: Into::<u64>::into(value.handle.native_pixmap.modifier).to_string(),
                supports_zero_copy_web_gpu_import: true, // TODO: 判定する？
            },
        };

        #[cfg(target_os = "windows")]
        let handle = SharedTextureHandle { nt_handle: None }; // TODO

        #[cfg(target_os = "macos")]
        let handle = SharedTextureHandle { io_surface: None }; // TODO

        let pixel_format = match value.pixel_format {
            DrmFourcc::Abgr8888 => "rgba", // AB24?
            DrmFourcc::Argb8888 => "bgra",
            DrmFourcc::Xbgr8888 => "rgba",
            DrmFourcc::Xrgb8888 => "bgra",
            DrmFourcc::Abgr16161616f => "rgbaf16",
            _ => panic!("Unsupported pixel format: {:?}", value.pixel_format),
        }
        .to_string();

        Self {
            widget_type: "frame".to_string(),
            // pixel_format,
            pixel_format: "rgbaf16".to_string(),
            coded_size: Size {
                width: value.width,
                height: value.height,
            },
            // https://www.electronjs.org/docs/latest/api/structures/color-space#common-colorspace-definitions
            // TODO: 動的な変更
            color_space: ColorSpace {
                primaries: "bt709".to_string(),
                transfer: "srgb".to_string(),
                matrix: "rgb".to_string(),
                range: "full".to_string(),
            },
            visible_rect: Rectangle {
                x: 0,
                y: 0,
                width: value.width,
                height: value.height,
            },
            content_rect: Rectangle {
                x: 0,
                y: 0,
                width: value.width,
                height: value.height,
            },
            timestamp: 0, // TODO
            metadata: None,
            handle,
        }
    }
}

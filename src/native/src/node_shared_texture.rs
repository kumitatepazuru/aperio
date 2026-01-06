// gpu_utilは独立したcrateであるため、Aperio本体にはnapi-rsが変換できる型にするために再度同じものを用意する

use std::os::fd::RawFd;

#[cfg(target_os = "linux")]
use gpu_util::texture_to_native::linux::{SharedTextureHandle, SharedTexturePlane};

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

#[napi(object)]
pub struct NodeSharedTexturePlane {
    #[napi(ts_type = "number")]
    pub fd: RawFd,
    pub stride: u32,
    pub offset: u32,
    pub size: u32,
}

#[napi(object)]
pub struct NodeSharedTextureHandleNativePixmap {
    pub planes: Vec<NodeSharedTexturePlane>,
    pub modifier: String,
    pub supports_zero_copy_web_gpu_import: bool,
}

// https://www.electronjs.org/docs/latest/api/structures/shared-texture-handle
#[napi(object)]
#[cfg(target_os = "linux")]
pub struct NodeSharedTextureHandle {
    pub native_pixmap: NodeSharedTextureHandleNativePixmap,
}

#[napi(object)]
#[cfg(target_os = "windows")]
pub struct NodeSharedTextureHandle {
    pub nt_handle: None, // TODO
}

#[napi(object)]
#[cfg(target_os = "macos")]
pub struct NodeSharedTextureHandle {
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
    pub handle: NodeSharedTextureHandle,
}

impl From<NodeSharedTextureHandle> for SharedTextureHandle {
    fn from(node_handle: NodeSharedTextureHandle) -> Self {
        #[cfg(target_os = "linux")]
        {
            use gpu_util::texture_to_native::linux::SharedTextureHandleNativePixmap;

            let planes = node_handle
                .native_pixmap
                .planes
                .into_iter()
                .map(|plane| SharedTexturePlane {
                    fd: plane.fd,
                    stride: plane.stride,
                    offset: plane.offset,
                    size: plane.size,
                })
                .collect();

            SharedTextureHandle {
                native_pixmap: SharedTextureHandleNativePixmap {
                    planes,
                    modifier: node_handle.native_pixmap.modifier,
                },
            }
        }

        #[cfg(target_os = "windows")]
        {
            unimplemented!("Windows SharedTextureHandle is not implemented yet");
        }

        #[cfg(target_os = "macos")]
        {
            unimplemented!("macOS SharedTextureHandle is not implemented yet");
        }
    }
}

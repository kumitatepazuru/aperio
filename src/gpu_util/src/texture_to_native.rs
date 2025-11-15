use std::os::fd::RawFd;

use drm_fourcc::{DrmFourcc, DrmModifier};

pub mod linux;

pub struct SharedTexturePlane {
    pub fd: RawFd,
    pub stride: u32,
    pub offset: u32,
    pub size: u32,
}

pub struct SharedTextureHandleNativePixmap {
    pub planes: Vec<SharedTexturePlane>,
    pub modifier: DrmModifier,
}

pub struct SharedTextureHandle {
    #[cfg(target_os = "linux")]
    pub native_pixmap: SharedTextureHandleNativePixmap,
    #[cfg(target_os = "windows")]
    pub nt_handle: None, // TODO
    #[cfg(target_os = "macos")]
    pub io_surface: None, // TODO
}

pub struct OffscreenSharedTextureInfo {
    pub handle: SharedTextureHandle,
    pub pixel_format: DrmFourcc,
    pub width: u32,
    pub height: u32,
}

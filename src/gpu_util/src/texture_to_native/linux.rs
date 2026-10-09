use std::os::fd::RawFd;
use std::sync::Arc;

use anyhow::bail;
use anyhow::Result;

use crate::{image_generator::ImageGenerator, SharedTextureFormat};

pub struct SharedTextureHandle {
    pub native_pixmap: SharedTextureHandleNativePixmap,
}

pub struct SharedTextureHandleNativePixmap {
    pub planes: Vec<SharedTexturePlane>,
    pub modifier: String,
}

pub struct SharedTexturePlane {
    pub fd: RawFd,
    pub stride: u32,
    pub offset: u32,
    pub size: u32,
}

/// TODO: dmabufのインポート自体は元々生のashで実装されていたため移植自体は難しくないが、
/// インポートしたテクスチャへ最終的に書き込むblit_texture_with_render_pass
/// がまだ`rhi`側に無いため、この機能はまだ提供できない。
/// ラスターパイプライン対応を実装したタイミングで、あわせてこちらも復活させる。
pub fn attach_texture_to_shared_texture(
    _shared_handle: &SharedTextureHandle,
    _format: &SharedTextureFormat,
    _source_texture: &crate::rhi::Texture,
    _generator: &ImageGenerator,
) -> Result<()> {
    bail!(
        "attach_texture_to_shared_texture (Linux dmabuf interop) is not yet implemented on \
         the Vulkan RHI backend (pending graphics/raster pipeline support)"
    )
}

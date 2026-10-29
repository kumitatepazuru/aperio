use std::os::fd::RawFd;

use anyhow::{Context, Result};

use crate::rhi::{DmaBufPlane, ExternalTextureHandle, TextureFormat, TextureUsage};
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

/// SharedTextureHandleのdmabufをVulkanのテクスチャとして取り込み、
/// そこへsource_textureを描き写す。処理はこの関数が戻る時点で完了している(同期submit)。
///
/// # Arguments
/// * `shared_handle` - dmabufのハンドル情報を含むSharedTextureHandle。dmabufは
///   source_textureと同じサイズ・`format`に対応するフォーマットで作られている必要がある。
/// * `source_texture` - 書き込み元のテクスチャ
/// * `generator` - blitパイプラインを持つImageGenerator
pub fn attach_texture_to_shared_texture(
    shared_handle: &SharedTextureHandle,
    format: &SharedTextureFormat,
    source_texture: &crate::rhi::Texture,
    generator: &ImageGenerator,
) -> Result<()> {
    let pixmap = &shared_handle.native_pixmap;
    let modifier: u64 = pixmap
        .modifier
        .parse()
        .context("Failed to parse the DRM modifier")?;
    let format = match format {
        SharedTextureFormat::Rgba16Float => TextureFormat::Rgba16Float,
        SharedTextureFormat::Bgra8Unorm => TextureFormat::Bgra8Unorm,
    };

    let destination = generator
        .device
        .import_external_texture(
            &ExternalTextureHandle::DmaBuf {
                planes: pixmap
                    .planes
                    .iter()
                    .map(|p| DmaBufPlane {
                        fd: p.fd,
                        stride: p.stride,
                        offset: p.offset,
                        size: p.size,
                    })
                    .collect(),
                modifier,
            },
            source_texture.width(),
            source_texture.height(),
            format,
            TextureUsage::COLOR_TARGET,
        )
        .context("Failed to import the dmabuf into Vulkan")?;

    generator.blit_to_texture(source_texture, &destination)
}

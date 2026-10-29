use std::ffi::c_void;

use anyhow::{Context, Result};
use windows::Win32::Foundation::HANDLE;

use crate::rhi::{ExternalTextureHandle, TextureFormat, TextureUsage};
use crate::{image_generator::ImageGenerator, SharedTextureFormat};

pub struct SharedTextureHandle {
    pub nt_handle: Vec<u8>,
}

fn bytes_to_handle(bytes: &[u8]) -> Result<HANDLE> {
    let s = std::mem::size_of::<usize>();
    if bytes.len() < s {
        anyhow::bail!(
            "Invalid handle bytes size: expected at least {s}, got {}",
            bytes.len()
        );
    }
    let mut array = [0u8; std::mem::size_of::<usize>()];
    array.copy_from_slice(&bytes[..s]);
    let v = usize::from_ne_bytes(array);
    Ok(HANDLE(v as *mut c_void))
}

/// SharedTextureHandleのNTハンドル(D3D12リソース)をVulkanのテクスチャとして取り込み、
/// そこへsource_textureを描き写す。
///
/// D3D12側は共有ハンドルを提供するだけで、書き込みはすべてVulkanが行う。
/// 処理はこの関数が戻る時点で完了している(同期submit)ので、呼び出し側のD3D12は
/// そのまま共有リソースを読める。
///
/// # Arguments
/// * `shared_handle` - NTハンドル情報を含むSharedTextureHandle。D3D12リソースは
///   source_textureと同じサイズ・`format`に対応するフォーマットで作られている必要がある。
/// * `source_texture` - 書き込み元のテクスチャ
/// * `generator` - blitパイプラインを持つImageGenerator
pub fn attach_texture_to_shared_texture(
    shared_handle: &SharedTextureHandle,
    format: &SharedTextureFormat,
    source_texture: &crate::rhi::Texture,
    generator: &ImageGenerator,
) -> Result<()> {
    let handle = bytes_to_handle(&shared_handle.nt_handle)?;
    let format = match format {
        SharedTextureFormat::Rgba16Float => TextureFormat::Rgba16Float,
        SharedTextureFormat::Bgra8Unorm => TextureFormat::Bgra8Unorm,
    };

    let destination = generator
        .device
        .import_external_texture(
            &ExternalTextureHandle::D3D12Resource {
                nt_handle: handle.0 as usize,
            },
            source_texture.width(),
            source_texture.height(),
            format,
            TextureUsage::COLOR_TARGET,
        )
        .context("Failed to import the shared D3D12 texture into Vulkan")?;

    generator.blit_to_texture(source_texture, &destination)
}

use std::ffi::c_void;

use anyhow::{bail, Result};
use windows::Win32::Foundation::HANDLE;

use crate::{image_generator::ImageGenerator, SharedTextureFormat};

pub struct SharedTextureHandle {
    pub nt_handle: Vec<u8>,
}

#[allow(dead_code)]
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

/// TODO: このプラットフォームでは、以前はwgpu::hal::dx12経由で
/// D3D12のOpenSharedHandleを呼び出していたが、実行エンジンをVulkanへ
/// 移行したため使えなくなった。今後は逆に、Vulkan自身が
/// VK_KHR_external_memory_win32経由でこのNTハンドル(D3D12リソース)を
/// インポートし、直接書き込む実装に置き換える必要がある
pub fn attach_texture_to_shared_texture(
    _shared_handle: &SharedTextureHandle,
    _format: &SharedTextureFormat,
    _source_texture: &crate::rhi::Texture,
    _generator: &ImageGenerator,
) -> Result<()> {
    bail!(
        "attach_texture_to_shared_texture (Windows D3D12 interop) is not yet implemented on \
         the Vulkan RHI backend"
    )
}

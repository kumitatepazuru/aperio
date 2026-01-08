use std::ffi::c_void;

use anyhow::{Context, Result};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Direct3D12::ID3D12Resource;

use crate::{
    image_generator::ImageGenerator,
    texture_to_native::post_pipeline::blit_texture_with_render_pass,
};

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

/// SharedTextureHandleのNTハンドルをD3D12を使いwgpu::Textureに変換し、
/// render passを使ってSharedTextureHandleのTextureにsource_textureを書き込む
///
/// # Arguments
/// * `shared_handle` - NTハンドル情報を含むSharedTextureHandle
/// * `source_texture` - 書き込み元のwgpu::Texture (f32フォーマット)
/// * `generator` - render pipelineを持つImageGenerator
///
/// # Safety
/// この関数はD3D12のunsafeなAPIを使用します。
pub fn attach_texture_to_shared_texture(
    shared_handle: &SharedTextureHandle,
    format: &str,
    source_texture: &wgpu::Texture,
    generator: &ImageGenerator,
) -> Result<()> {
    // source_textureからサイズを取得
    let width = source_texture.width();
    let height = source_texture.height();

    // NTハンドルからwgpuテクスチャを作成
    let destination_texture =
        create_texture_from_shared_handle(shared_handle, generator, &format, width, height)?;

    // render passを使ってsource_textureをdestination_textureに書き込む
    blit_texture_with_render_pass(source_texture, &format, &destination_texture, generator)?;

    generator.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    })?;

    Ok(())
}

/// NTハンドルからwgpu::Textureを作成する
fn create_texture_from_shared_handle(
    shared_handle: &SharedTextureHandle,
    generator: &ImageGenerator,
    shared_handle_format: &str,
    width: u32,
    height: u32,
) -> Result<wgpu::Texture> {
    // bytesからHANDLEに変換
    let handle = bytes_to_handle(&shared_handle.nt_handle)?;

    // formatからテクスチャフォーマットを決定
    let tex_format = match shared_handle_format {
        "rgbaf16" => wgpu::TextureFormat::Rgba16Float,
        "bgra" => wgpu::TextureFormat::Bgra8Unorm,
        _ => anyhow::bail!(
            "Unsupported shared texture format: {}",
            shared_handle_format
        ),
    };

    unsafe {
        // D3D12 HALへのアクセスを取得
        let device_guard = generator
            .device
            .as_hal::<wgpu::hal::api::Dx12>()
            .context("Failed to get D3D12 device")?;

        // D3D12デバイスを取得してOpenSharedHandleを呼び出す
        let raw_device = device_guard.raw_device();

        // OpenSharedHandleでID3D12Resourceを取得
        let mut d3d12_resource: Option<ID3D12Resource> = None;
        raw_device
            .OpenSharedHandle(handle, &mut d3d12_resource)
            .context("Failed to open shared handle")?;

        let d3d12_resource =
            d3d12_resource.context("OpenSharedHandle returned None for ID3D12Resource")?;

        // deviceガードをドロップしてからtexture_from_rawを呼ぶ
        drop(device_guard);

        // ID3D12Resourceからwgpu-hal Textureを作成
        let hal_texture = wgpu::hal::dx12::Device::texture_from_raw(
            d3d12_resource,
            tex_format,
            wgpu::TextureDimension::D2,
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            1, // mip_level_count
            1, // sample_count
        );

        // HALテクスチャをwgpuテクスチャに変換
        let texture = generator
            .device
            .create_texture_from_hal::<wgpu::hal::api::Dx12>(
                hal_texture,
                &wgpu::TextureDescriptor {
                    label: Some("Shared Texture from NT Handle"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: tex_format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
            );

        Ok(texture)
    }
}

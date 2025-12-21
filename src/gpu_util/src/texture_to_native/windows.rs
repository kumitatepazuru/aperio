use std::ffi::c_void;

use anyhow::{Context, Result};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Direct3D12::ID3D12Resource;

use crate::{
    image_generator::ImageGenerator, texture_to_native::post_pipeline::execute_f32_to_f16_pipeline,
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
/// ImageGeneratorにあるf32_to_f16のパイプラインを使ってSharedTextureHandleのTextureにwgpu::Textureを書き込む
///
/// # Arguments
/// * `shared_handle` - NTハンドル情報を含むSharedTextureHandle
/// * `source_texture` - 書き込み元のwgpu::Texture (f32フォーマット)
/// * `generator` - f32_to_f16パイプラインを持つImageGenerator
///
/// # Safety
/// この関数はD3D12のunsafeなAPIを使用します。
pub fn attach_texture_to_shared_texture(
    shared_handle: &SharedTextureHandle,
    source_texture: &wgpu::Texture,
    generator: &ImageGenerator,
) -> Result<()> {
    // source_textureからサイズを取得
    let width = source_texture.width();
    let height = source_texture.height();

    // NTハンドルからwgpuテクスチャを作成
    let destination_texture =
        create_texture_from_shared_handle(shared_handle, generator, width, height)?;

    // f32_to_f16パイプラインを使ってsource_textureをdestination_textureに書き込む
    execute_f32_to_f16_pipeline(source_texture, &destination_texture, generator)?;

    Ok(())
}

/// NTハンドルからwgpu::Textureを作成する
fn create_texture_from_shared_handle(
    shared_handle: &SharedTextureHandle,
    generator: &ImageGenerator,
    width: u32,
    height: u32,
) -> Result<wgpu::Texture> {
    // bytesからHANDLEに変換
    let handle = bytes_to_handle(&shared_handle.nt_handle)?;

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
            wgpu::TextureFormat::Rgba16Float,
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
                    format: wgpu::TextureFormat::Rgba16Float,
                    usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                },
            );

        Ok(texture)
    }
}

use std::os::fd::RawFd;

use anyhow::{bail, Context, Result};
use ash::vk;

use crate::image_generator::ImageGenerator;

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

fn dup_fd(fd: i32) -> Result<i32> {
    // dup() は newfd を返す。失敗時は -1
    let newfd = unsafe { libc::dup(fd) };
    if newfd < 0 {
        return Err(std::io::Error::last_os_error())
            .context("dup(fd) failed")
            .map_err(Into::into);
    }
    Ok(newfd)
}

/// SharedTextureHandleのdmabufをVulkanを使いwgpu::Textureに変換し、
/// ImageGeneratorにあるf32_to_f16またはf32_to_bgraのパイプラインを使ってSharedTextureHandleのTextureにwgpu::Textureを書き込む
///
/// # Arguments
/// * `shared_handle` - dmabufのハンドル情報を含むSharedTextureHandle
/// * `source_texture` - 書き込み元のwgpu::Texture (f32フォーマット)
/// * `generator` - ImageGenerator
///
/// # Safety
/// この関数はVulkanのunsafeなAPIを使用します。
pub fn attach_texture_to_shared_texture(
    shared_handle: &SharedTextureHandle,
    format: String,
    source_texture: &wgpu::Texture,
    generator: &ImageGenerator,
) -> Result<()> {
    // source_textureからサイズを取得
    let width = source_texture.width();
    let height = source_texture.height();

    // dmabufからwgpuテクスチャを作成
    let destination_texture =
        create_texture_from_dmabuf(shared_handle, generator, &format, width, height)?;

    // 少なくとも、Linux x11 + Vulkan + NVIDIA GPUの場合、rgbaf16が未対応のようなのでformatを見てf16かu8かを判定する
    // f32_to_f16またはf32_to_bgraパイプラインを使ってsource_textureをdestination_textureに書き込む
    execute_f32_to_shared_texture_pipeline(
        source_texture,
        &destination_texture,
        &format,
        generator,
    )?;

    Ok(())
}

/// dmabufからwgpu::Textureを作成する
fn create_texture_from_dmabuf(
    shared_handle: &SharedTextureHandle,
    generator: &ImageGenerator,
    shared_handle_format: &String,
    width: u32,
    height: u32,
) -> Result<wgpu::Texture> {
    let planes = &shared_handle.native_pixmap.planes;
    if planes.is_empty() {
        anyhow::bail!("No planes in shared texture handle");
    }

    let modifier: u64 = shared_handle
        .native_pixmap
        .modifier
        .parse()
        .context("Failed to parse modifier")?;

    // planeのレイアウト情報を事前に計算
    let plane_layouts: Vec<vk::SubresourceLayout> = planes
        .iter()
        .map(|p| vk::SubresourceLayout {
            offset: p.offset as u64,
            size: p.size as u64,
            row_pitch: p.stride as u64,
            array_pitch: 0,
            depth_pitch: 0,
        })
        .collect();

    // Vulkan HAL経由でdmabufをインポート
    unsafe {
        let device_guard = generator
            .device
            .as_hal::<wgpu::hal::api::Vulkan>()
            .context("Failed to get Vulkan device")?;

        // VkExternalMemoryImageCreateInfo を使用してdmabuf対応のイメージを作成
        let drm_format_modifier_info = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
            .drm_format_modifier(modifier)
            .plane_layouts(&plane_layouts);

        let external_memory_info = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);

        let vk_format = match shared_handle_format.as_str() {
            "rgbaf16" => vk::Format::R16G16B16A16_SFLOAT,
            "bgra" => vk::Format::R8G8B8A8_UNORM,
            _ => {
                bail!(
                    "Unsupported shared texture format: {}",
                    shared_handle_format
                );
            }
        };

        let image_create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk_format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        // p_nextチェーンを構築
        let drm_info = drm_format_modifier_info;
        let mut ext_mem_info = external_memory_info;
        ext_mem_info.p_next = &drm_info as *const _ as *const std::ffi::c_void;
        let mut img_create_info = image_create_info;
        img_create_info.p_next = &ext_mem_info as *const _ as *const std::ffi::c_void;

        let raw_device = device_guard.raw_device();
        let image = raw_device
            .create_image(&img_create_info, None)
            .context("Failed to create Vulkan image")?;

        // メモリ要件を取得
        let mem_requirements = raw_device.get_image_memory_requirements(image);

        // dmabufからメモリをインポート
        let imported_fd = dup_fd(planes[0].fd)?;
        let import_memory_fd_info = vk::ImportMemoryFdInfoKHR::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
            .fd(imported_fd);

        let memory_allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_requirements.size)
            .memory_type_index(
                find_memory_type_index(
                    &device_guard,
                    mem_requirements.memory_type_bits,
                    vk::MemoryPropertyFlags::DEVICE_LOCAL,
                )
                .context("Failed to find suitable memory type")?,
            );

        // p_nextチェーンを構築
        let import_fd_info = import_memory_fd_info;
        let mut mem_alloc_info = memory_allocate_info;
        mem_alloc_info.p_next = &import_fd_info as *const _ as *const std::ffi::c_void;

        let memory = raw_device
            .allocate_memory(&mem_alloc_info, None)
            .context("Failed to allocate memory")?;

        raw_device
            .bind_image_memory(image, memory, 0)
            .context("Failed to bind image memory")?;

        let tex_format = match shared_handle_format.as_str() {
            "rgbaf16" => wgpu::TextureFormat::Rgba16Float,
            "bgra" => wgpu::TextureFormat::Rgba8Unorm,
            _ => {
                bail!(
                    "Unsupported shared texture format: {}",
                    shared_handle_format
                );
            }
        };

        // wgpu-halのTextureDescを作成
        let hal_texture_desc = wgpu::hal::TextureDescriptor {
            label: Some("Shared Texture from dmabuf"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: tex_format,
            usage: wgpu::TextureUses::STORAGE_READ_WRITE | wgpu::TextureUses::COPY_DST,
            memory_flags: wgpu::hal::MemoryFlags::empty(),
            view_formats: vec![],
        };

        // raw_deviceのクローン（所有権の問題を回避）
        let raw_device_clone = raw_device.clone();

        // メモリとイメージを解放するためのコールバック
        let drop_callback: wgpu::hal::DropCallback = Box::new(move || {
            // 注意: このクロージャはテクスチャがドロップされた時に呼ばれる
            // 安全にリソースを解放する
            raw_device_clone.destroy_image(image, None);
            raw_device_clone.free_memory(memory, None);
        });

        let hal_texture =
            device_guard.texture_from_raw(image, &hal_texture_desc, Some(drop_callback));

        // deviceガードをドロップしてからcreate_texture_from_halを呼ぶ
        drop(device_guard);

        // HALテクスチャをwgpuテクスチャに変換
        let texture = generator
            .device
            .create_texture_from_hal::<wgpu::hal::api::Vulkan>(
                hal_texture,
                &wgpu::TextureDescriptor {
                    label: Some("Shared Texture from dmabuf"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: tex_format,
                    usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                },
            );

        Ok(texture)
    }
}

/// 適切なメモリタイプインデックスを見つける
unsafe fn find_memory_type_index(
    device: &<wgpu::hal::api::Vulkan as wgpu::hal::Api>::Device,
    type_bits: u32,
    properties: vk::MemoryPropertyFlags,
) -> Option<u32> {
    let memory_properties = device
        .shared_instance()
        .raw_instance()
        .get_physical_device_memory_properties(device.raw_physical_device());

    for i in 0..memory_properties.memory_type_count {
        if (type_bits & (1 << i)) != 0
            && (memory_properties.memory_types[i as usize].property_flags & properties)
                == properties
        {
            return Some(i);
        }
    }
    None
}

/// f32_to_shared_textureパイプラインを実行してsource_textureをdestination_textureに書き込む
fn execute_f32_to_shared_texture_pipeline(
    source_texture: &wgpu::Texture,
    destination_texture: &wgpu::Texture,
    destination_format: &String,
    generator: &ImageGenerator,
) -> Result<()> {
    // source_textureからサイズを取得
    let width = source_texture.width();
    let height = source_texture.height();

    let source_view = source_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let destination_view = destination_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let pipeline = match destination_format.as_str() {
        "rgbaf16" => &generator.f32_to_f16_pipeline,
        "bgra" => &generator.f32_to_bgra_pipeline,
        _ => {
            bail!(
                "Unsupported destination texture format for f32_to_shared_texture: {}",
                destination_format
            );
        }
    };

    // BindGroupを作成
    let bind_group = generator
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("f32_to_shared_tex Bind Group"),
            layout: &pipeline.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&destination_view),
                },
            ],
        });

    // CommandEncoderを作成してComputePassを実行
    let mut encoder = generator
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("f32_to_shared_tex Command Encoder"),
        });

    {
        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("f32_to_shared_tex Compute Pass"),
            timestamp_writes: None,
        });

        compute_pass.set_pipeline(&pipeline.pipeline);
        compute_pass.set_bind_group(0, &bind_group, &[]);

        // ワークグループサイズは16x16 (wgslに合わせる)
        let workgroup_count_x = (width + 15) / 16;
        let workgroup_count_y = (height + 15) / 16;
        compute_pass.dispatch_workgroups(workgroup_count_x, workgroup_count_y, 1);
    }

    generator.queue.submit(std::iter::once(encoder.finish()));

    generator.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    })?;

    Ok(())
}

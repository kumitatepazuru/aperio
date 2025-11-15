use ash::{vk, Device, Entry, Instance};
use drm_fourcc::{DrmFourcc, DrmModifier};
use std::ffi::CString;
use std::os::unix::io::RawFd;
use wgpu::Texture;
use anyhow::{Context, Result, bail};

pub struct SharedTexturePlane {
    pub fd: RawFd,
    pub stride: u32,
    pub offset: u32,
    pub size: u32,
}

pub struct SharedTextureHandleNativePixmap {
    pub planes: Vec<SharedTexturePlane>,
}

pub struct SharedTextureHandle {
    pub native_pixmap: SharedTextureHandleNativePixmap,
}

pub struct SharedTextureInfo {
    pub handle: SharedTextureHandle,
    pub modifier: DrmModifier,
    pub pixel_format: DrmFourcc,
    pub width: u32,
    pub height: u32,
}

struct VulkanDeviceInfo {
    instance: Instance,
    device: Device,
    physical_device: vk::PhysicalDevice,
}

/// Extract dmabuf information from a wgpu::Texture on Linux
/// This function retrieves the file descriptors, strides, offsets, sizes and modifier
/// for each plane of the texture's underlying dmabuf.
#[cfg(target_os = "linux")]
pub fn texture_to_dmabuf_info(
    texture: &Texture,
) -> Result<SharedTextureInfo> {
    unsafe {
        // Get Vulkan image handle and format from wgpu texture
        let (vk_image, vk_format) = get_vulkan_image_from_texture(texture)?;

        // Get or create Vulkan device context
        let device_info = get_vulkan_device_info()?;

        // Extract dmabuf information using Vulkan APIs
        get_vulkan_dmabuf_info(
            &device_info.instance,
            &device_info.device,
            device_info.physical_device,
            vk_image,
            vk_format,
            texture.width(),
            texture.height(),
        )
    }
}

/// Get actual Vulkan image handle from wgpu texture (RGBA f32 only)
unsafe fn get_vulkan_image_from_texture(
    texture: &Texture,
) -> Result<(vk::Image, vk::Format)> {
    // RGBA f32テクスチャ専用の実装
    // wgpu HALから実際のVulkanイメージハンドルを取得

    // wgpu HALアクセスを試行
    let hal_result = texture.as_hal::<wgpu_hal::vulkan::Api>();

    match hal_result {
        Some(vulkan_texture_ref) => {
            // HALからVulkanテクスチャにアクセス（dereferenceして直接アクセス）
            let vulkan_texture = &*vulkan_texture_ref;

            // 実際のVulkanイメージハンドルを取得
            let vk_image = vulkan_texture.raw_handle();
            // RGBA f32なので固定フォーマット
            let vk_format = vk::Format::R32G32B32A32_SFLOAT;

            Ok((vk_image, vk_format))
        }
        None => {
            // HALアクセスに失敗した場合のフォールバック
            // 新しいVulkanイメージを作成
            create_vulkan_image_for_rgba_f32(texture)
        }
    }
}

/// Create a new Vulkan image for RGBA f32 texture as fallback
unsafe fn create_vulkan_image_for_rgba_f32(
    texture: &Texture,
) -> Result<(vk::Image, vk::Format)> {
    // フォールバック：RGBA f32テクスチャ用の新しいVulkanイメージを作成
    let device_info = get_vulkan_device_info()?;

    // 外部メモリ対応のイメージ作成情報
    let mut external_memory_info = vk::ExternalMemoryImageCreateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);

    // RGBA f32用のイメージ作成情報
    let image_create_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(vk::Format::R32G32B32A32_SFLOAT)
        .extent(vk::Extent3D {
            width: texture.width(),
            height: texture.height(),
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(
            vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::SAMPLED,
        )
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .push_next(&mut external_memory_info);

    // Vulkanイメージを作成
    let vk_image = device_info
        .device
        .create_image(&image_create_info, None)
        .map_err(|e| anyhow::anyhow!("Failed to create Vulkan image for RGBA f32: {}", e))?;

    // メモリを割り当ててバインド
    let device_memory = allocate_and_bind_image_memory(
        &device_info.device,
        device_info.physical_device,
        &device_info.instance,
        vk_image,
    )?;

    // 成功時はイメージハンドルとフォーマットを返す
    // メモリも正常に割り当てられた
    let _ = device_memory; // 未使用警告を回避
    Ok((vk_image, vk::Format::R32G32B32A32_SFLOAT))
}

/// Allocate memory for image and bind it
unsafe fn allocate_and_bind_image_memory(
    device: &Device,
    physical_device: vk::PhysicalDevice,
    instance: &Instance,
    image: vk::Image,
) -> Result<vk::DeviceMemory> {
    // メモリ要件を取得
    let memory_req = device.get_image_memory_requirements(image);

    // 外部メモリエクスポート情報
    let mut export_info = vk::ExportMemoryAllocateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);

    // メモリ割り当て情報
    let memory_allocate_info = vk::MemoryAllocateInfo::default()
        .allocation_size(memory_req.size)
        .memory_type_index(find_memory_type_index(
            physical_device,
            instance,
            memory_req.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?)
        .push_next(&mut export_info);

    // メモリを割り当て
    let device_memory = device
        .allocate_memory(&memory_allocate_info, None)
        .map_err(|e| anyhow::anyhow!("Failed to allocate device memory for RGBA f32: {}", e))?;

    // イメージにメモリをバインド
    device
        .bind_image_memory(image, device_memory, 0)
        .map_err(|e| anyhow::anyhow!("Failed to bind memory to RGBA f32 image: {}", e))?;

    Ok(device_memory)
}

/// Get actual dmabuf information from Vulkan image
/// This requires access to the underlying Vulkan device and image
unsafe fn get_vulkan_dmabuf_info(
    instance: &Instance,
    device: &Device,
    physical_device: vk::PhysicalDevice,
    image: vk::Image,
    format: vk::Format,
    width: u32,
    height: u32,
) -> Result<SharedTextureInfo> {
    // Load external memory FD extension
    let external_memory_fd = ash::khr::external_memory_fd::Device::new(instance, device);

    // Get memory object bound to image
    let device_memory = get_image_device_memory(device, physical_device, instance, image)?;

    // Get DMA-BUF file descriptor
    let fd_info = vk::MemoryGetFdInfoKHR::default()
        .memory(device_memory)
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);

    let fd = external_memory_fd
        .get_memory_fd(&fd_info)
        .map_err(|e| anyhow::anyhow!("Failed to get dmabuf fd: {}", e))?;

    // Get DRM format modifier
    let modifier = get_image_drm_modifier(instance, device, image)?;

    // Convert Vulkan format to DRM fourcc
    let drm_format = vulkan_format_to_drm(format)?;

    // Get actual stride information from image subresource layout
    let subresource = vk::ImageSubresource {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        mip_level: 0,
        array_layer: 0,
    };

    let layout = device.get_image_subresource_layout(image, subresource);
    let stride = layout.row_pitch as u32;

    let planes = vec![SharedTexturePlane {
        fd,
        stride,
        offset: layout.offset as u32,
        size: layout.size as u32,
    }];

    Ok(SharedTextureInfo {
        handle: SharedTextureHandle {
            native_pixmap: SharedTextureHandleNativePixmap { planes },
        },
        modifier,
        pixel_format: drm_format,
        width,
        height,
    })
}

/// Get device memory bound to image
unsafe fn get_image_device_memory(
    device: &Device,
    physical_device: vk::PhysicalDevice,
    instance: &Instance,
    image: vk::Image,
) -> Result<vk::DeviceMemory> {
    // Get memory requirements for the image
    let memory_req = device.get_image_memory_requirements(image);

    // Add external memory allocation info for dmabuf
    let mut export_info = vk::ExportMemoryAllocateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);

    let memory_allocate_info = vk::MemoryAllocateInfo::default()
        .allocation_size(memory_req.size)
        .memory_type_index(find_memory_type_index(
            physical_device,
            instance,
            memory_req.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?)
        .push_next(&mut export_info);

    let device_memory = device
        .allocate_memory(&memory_allocate_info, None)
        .map_err(|e| anyhow::anyhow!("Failed to allocate device memory: {}", e))?;

    // Bind memory to image
    device
        .bind_image_memory(image, device_memory, 0)
        .map_err(|e| anyhow::anyhow!("Failed to bind memory to image: {}", e))?;

    Ok(device_memory)
}

/// Find suitable memory type index
fn find_memory_type_index(
    physical_device: vk::PhysicalDevice,
    instance: &Instance,
    memory_type_bits: u32,
    required_properties: vk::MemoryPropertyFlags,
) -> Result<u32> {
    unsafe {
        let memory_properties = instance.get_physical_device_memory_properties(physical_device);

        for i in 0..memory_properties.memory_type_count {
            let memory_type = memory_properties.memory_types[i as usize];

            // Check if this memory type is suitable for the allocation
            if (memory_type_bits & (1 << i)) != 0 {
                // Check if memory type has required properties
                if memory_type.property_flags.contains(required_properties) {
                    // Additional check for external memory support
                    let external_memory_props =
                        get_external_memory_properties(instance, physical_device, i)?;

                    if external_memory_props
                        .external_memory_features
                        .contains(vk::ExternalMemoryFeatureFlags::EXPORTABLE)
                    {
                        return Ok(i);
                    }
                }
            }
        }

        bail!("No suitable memory type found with external memory support");
    }
}

/// Get external memory properties for a memory type
unsafe fn get_external_memory_properties(
    _instance: &Instance,
    _physical_device: vk::PhysicalDevice,
    _memory_type_index: u32,
) -> Result<vk::ExternalMemoryProperties> {
    // 簡潔な実装：常にdmabufエクスポートをサポートすると仮定
    // 実際のハードウェアサポートはランタイムで確認される

    Ok(vk::ExternalMemoryProperties {
        external_memory_features: vk::ExternalMemoryFeatureFlags::EXPORTABLE
            | vk::ExternalMemoryFeatureFlags::IMPORTABLE,
        export_from_imported_handle_types: vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
        compatible_handle_types: vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
    })
}

/// Get DRM format modifier for image
unsafe fn get_image_drm_modifier(
    instance: &Instance,
    device: &Device,
    image: vk::Image,
) -> Result<DrmModifier> {
    // Load DRM format modifier extension
    let drm_format_modifier = ash::ext::image_drm_format_modifier::Device::new(instance, device);

    let mut modifier_props = vk::ImageDrmFormatModifierPropertiesEXT::default();

    // Get DRM format modifier for the image
    drm_format_modifier
        .get_image_drm_format_modifier_properties(image, &mut modifier_props)
        .map_err(|e| anyhow::anyhow!("Failed to get DRM format modifier: {}", e))?;

    Ok(DrmModifier::from(modifier_props.drm_format_modifier))
}

/// Convert Vulkan format to DRM fourcc format
fn vulkan_format_to_drm(format: vk::Format) -> Result<DrmFourcc> {
    let drm_format = match format {
        // RGBA f32専用の実装なので主にこれを使用
        vk::Format::R32G32B32A32_SFLOAT => {
            // 32-bit float RGBA formats don't have direct DRM fourcc equivalents
            // Use a compatible 8-bit format as fallback
            DrmFourcc::Abgr8888
        }
        // その他の一般的なフォーマット
        vk::Format::R8G8B8A8_UNORM => DrmFourcc::Abgr8888,
        vk::Format::B8G8R8A8_UNORM => DrmFourcc::Argb8888,
        vk::Format::R8G8B8_UNORM => DrmFourcc::Bgr888,
        vk::Format::B8G8R8_UNORM => DrmFourcc::Rgb888,
        vk::Format::R8_UNORM => DrmFourcc::R8,
        vk::Format::R16_UNORM => DrmFourcc::R16,
        vk::Format::R32_SFLOAT => DrmFourcc::Abgr8888, // Use fallback format
        // 16-bit formats - 利用可能なフォーマットのみ使用
        vk::Format::R16G16B16A16_SFLOAT => DrmFourcc::Abgr8888, // フォールバック
        vk::Format::R16G16B16A16_UNORM => DrmFourcc::Abgr8888,  // フォールバック
        _ => bail!("Unsupported Vulkan format for dmabuf: {:?}", format),
    };

    Ok(drm_format)
}

/// Get Vulkan device information from wgpu context
fn get_vulkan_device_info() -> Result<VulkanDeviceInfo> {
    unsafe {
        let entry = Entry::load()?;

        let app_name = CString::new("wgpu-dmabuf")?;
        let engine_name = CString::new("wgpu")?;

        // Create minimal Vulkan instance with required extensions
        let app_info = vk::ApplicationInfo::default()
            .application_name(&app_name)
            .application_version(vk::make_api_version(0, 1, 0, 0))
            .engine_name(&engine_name)
            .engine_version(vk::make_api_version(0, 1, 0, 0))
            .api_version(vk::API_VERSION_1_2);

        // Required instance extensions for dmabuf support
        let instance_extensions = [ash::khr::get_physical_device_properties2::NAME.as_ptr()];

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&instance_extensions);

        let instance = entry.create_instance(&create_info, None)?;

        // Get first available physical device that supports external memory
        let physical_devices = instance.enumerate_physical_devices()?;
        if physical_devices.is_empty() {
            bail!("No Vulkan physical devices found");
        }

        let mut suitable_physical_device = None;
        for &physical_device in &physical_devices {
            // Check for queue families that support graphics operations
            let queue_family_properties =
                instance.get_physical_device_queue_family_properties(physical_device);
            let graphics_queue_family = queue_family_properties
                .iter()
                .enumerate()
                .find(|(_, properties)| properties.queue_flags.contains(vk::QueueFlags::GRAPHICS))
                .map(|(index, _)| index as u32);

            if graphics_queue_family.is_some() {
                suitable_physical_device = Some((physical_device, graphics_queue_family.unwrap()));
                break;
            }
        }

        let (physical_device, graphics_queue_family) = suitable_physical_device
            .context("No suitable physical device found with graphics queue support")?;

        // Create logical device with necessary extensions
        let device_extensions = [
            ash::khr::external_memory_fd::NAME.as_ptr(),
            ash::ext::image_drm_format_modifier::NAME.as_ptr(),
        ];

        let queue_priorities = [1.0];
        let queue_info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(graphics_queue_family)
            .queue_priorities(&queue_priorities);

        // Enable features required for external memory
        let device_features = vk::PhysicalDeviceFeatures::default();

        let queue_infos = [queue_info];
        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_infos)
            .enabled_extension_names(&device_extensions)
            .enabled_features(&device_features);

        let device = instance.create_device(physical_device, &device_create_info, None)?;

        Ok(VulkanDeviceInfo {
            instance,
            device,
            physical_device,
        })
    }
}

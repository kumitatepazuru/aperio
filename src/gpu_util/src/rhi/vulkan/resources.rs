// Buffer/Texture/TextureView/Sampler とVulkanリソースの所有権(Dropで解放)。

use std::sync::Arc;

use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan::{AllocationCreateDesc, AllocationScheme};
use gpu_allocator::MemoryLocation;

use super::device::VulkanDevice;
use crate::rhi::TextureFormat;

/// rhi::TextureFormatとVulkanの生フォーマットとの相互変換。
pub(crate) fn to_vk_format(format: TextureFormat) -> vk::Format {
    match format {
        TextureFormat::Rgba8Unorm => vk::Format::R8G8B8A8_UNORM,
        TextureFormat::Bgra8Unorm => vk::Format::B8G8R8A8_UNORM,
        TextureFormat::Rgba16Float => vk::Format::R16G16B16A16_SFLOAT,
        TextureFormat::Rgba32Float => vk::Format::R32G32B32A32_SFLOAT,
        TextureFormat::R8Unorm => vk::Format::R8_UNORM,
        TextureFormat::Rg8Unorm => vk::Format::R8G8_UNORM,
        TextureFormat::R16Unorm => vk::Format::R16_UNORM,
        TextureFormat::Rg16Unorm => vk::Format::R16G16_UNORM,
    }
}

pub(crate) fn from_vk_format(format: vk::Format) -> Result<TextureFormat> {
    match format {
        vk::Format::R8G8B8A8_UNORM => Ok(TextureFormat::Rgba8Unorm),
        vk::Format::B8G8R8A8_UNORM => Ok(TextureFormat::Bgra8Unorm),
        vk::Format::R16G16B16A16_SFLOAT => Ok(TextureFormat::Rgba16Float),
        vk::Format::R32G32B32A32_SFLOAT => Ok(TextureFormat::Rgba32Float),
        vk::Format::R8_UNORM => Ok(TextureFormat::R8Unorm),
        vk::Format::R8G8_UNORM => Ok(TextureFormat::Rg8Unorm),
        vk::Format::R16_UNORM => Ok(TextureFormat::R16Unorm),
        vk::Format::R16G16_UNORM => Ok(TextureFormat::Rg16Unorm),
        other => anyhow::bail!("Unsupported Vulkan format for TextureFormat: {other:?}"),
    }
}

pub struct Buffer {
    device: Arc<VulkanDevice>,
    handle: vk::Buffer,
    allocation: Option<gpu_allocator::vulkan::Allocation>,
}

impl Buffer {
    pub(crate) fn handle(&self) -> vk::Buffer {
        self.handle
    }

    pub fn mapped_ptr(&self) -> Option<std::ptr::NonNull<std::ffi::c_void>> {
        self.allocation.as_ref().and_then(|a| a.mapped_ptr())
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe { self.device.device.destroy_buffer(self.handle, None) };
        if let Some(allocation) = self.allocation.take() {
            let _ = self.device.allocator.free(allocation);
        }
    }
}

impl std::fmt::Debug for Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Buffer")
            .field("handle", &self.handle)
            .finish()
    }
}

pub struct Texture {
    device: Arc<VulkanDevice>,
    handle: vk::Image,
    allocation: Option<gpu_allocator::vulkan::Allocation>,
    /// 外部メモリをインポートして作ったイメージ専用のメモリ(アロケータ管理外)。
    imported_memory: Option<vk::DeviceMemory>,
    format: vk::Format,
    extent: vk::Extent3D,
    /// 現在のイメージレイアウト(プールで使い回されるため自前で追跡する)。
    layout: std::sync::Mutex<vk::ImageLayout>,
}

impl Texture {
    /// 外部メモリをインポートしたイメージを包む。memoryとimageの所有権はTextureに移る。
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    pub(super) fn from_imported(
        device: Arc<VulkanDevice>,
        image: vk::Image,
        memory: vk::DeviceMemory,
        format: vk::Format,
        extent: vk::Extent3D,
    ) -> Self {
        Self {
            device,
            handle: image,
            allocation: None,
            imported_memory: Some(memory),
            format,
            extent,
            layout: std::sync::Mutex::new(vk::ImageLayout::UNDEFINED),
        }
    }

    pub(crate) fn handle(&self) -> vk::Image {
        self.handle
    }

    pub(crate) fn vk_format(&self) -> vk::Format {
        self.format
    }

    pub fn width(&self) -> u32 {
        self.extent.width
    }

    pub fn height(&self) -> u32 {
        self.extent.height
    }

    pub fn format(&self) -> TextureFormat {
        // 生成経路は常にTextureFormat由来のフォーマットしか使わない。
        from_vk_format(self.format).expect("Texture was created with a non-abstract Vulkan format")
    }

    pub(crate) fn extent(&self) -> vk::Extent3D {
        self.extent
    }

    /// 追跡中のレイアウトがnew_layoutと異なる場合のみバリアを発行して遷移する。
    pub(crate) fn transition(
        &self,
        device: &VulkanDevice,
        command_buffer: vk::CommandBuffer,
        new_layout: vk::ImageLayout,
        src_stage: vk::PipelineStageFlags,
        dst_stage: vk::PipelineStageFlags,
        src_access: vk::AccessFlags,
        dst_access: vk::AccessFlags,
    ) {
        let mut current = self.layout.lock().unwrap();
        if *current == new_layout {
            return;
        }

        let barrier = vk::ImageMemoryBarrier::default()
            .old_layout(*current)
            .new_layout(new_layout)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.handle)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .src_access_mask(src_access)
            .dst_access_mask(dst_access);

        unsafe {
            device.device.cmd_pipeline_barrier(
                command_buffer,
                src_stage,
                dst_stage,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );
        }

        *current = new_layout;
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        unsafe { self.device.device.destroy_image(self.handle, None) };
        if let Some(allocation) = self.allocation.take() {
            let _ = self.device.allocator.free(allocation);
        }
        if let Some(memory) = self.imported_memory.take() {
            unsafe { self.device.device.free_memory(memory, None) };
        }
    }
}

impl std::fmt::Debug for Texture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Texture")
            .field("handle", &self.handle)
            .field("format", &self.format)
            .field("extent", &self.extent)
            .finish()
    }
}

pub struct TextureView {
    device: Arc<VulkanDevice>,
    handle: vk::ImageView,
    /// vkImageViewはvkImageより先に破棄する必要があるため、所有権を保持する。
    texture: Arc<Texture>,
}

impl TextureView {
    pub(crate) fn handle(&self) -> vk::ImageView {
        self.handle
    }

    pub(crate) fn texture(&self) -> &Arc<Texture> {
        &self.texture
    }
}

impl Drop for TextureView {
    fn drop(&mut self) {
        unsafe { self.device.device.destroy_image_view(self.handle, None) };
    }
}

pub struct Sampler {
    device: Arc<VulkanDevice>,
    handle: vk::Sampler,
}

impl Sampler {
    pub(crate) fn handle(&self) -> vk::Sampler {
        self.handle
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        unsafe { self.device.device.destroy_sampler(self.handle, None) };
    }
}

impl VulkanDevice {
    pub(crate) fn create_vk_buffer(
        self: &Arc<Self>,
        size: u64,
        usage: vk::BufferUsageFlags,
        location: MemoryLocation,
        name: &str,
    ) -> Result<Arc<Buffer>> {
        let buffer_create_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let handle = unsafe { self.device.create_buffer(&buffer_create_info, None) }
            .context("Failed to create Vulkan buffer")?;
        let requirements = unsafe { self.device.get_buffer_memory_requirements(handle) };

        let allocation = self
            .allocator
            .allocate(&AllocationCreateDesc {
                name,
                requirements,
                location,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .inspect_err(|_| unsafe { self.device.destroy_buffer(handle, None) })?;

        unsafe {
            self.device
                .bind_buffer_memory(handle, allocation.memory(), allocation.offset())
        }
        .context("Failed to bind buffer memory")?;

        Ok(Arc::new(Buffer {
            device: self.clone(),
            handle,
            allocation: Some(allocation),
        }))
    }

    pub(crate) fn create_vk_texture(
        self: &Arc<Self>,
        width: u32,
        height: u32,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        name: &str,
    ) -> Result<Arc<Texture>> {
        let extent = vk::Extent3D {
            width,
            height,
            depth: 1,
        };

        // 用途ごとに必要なフォーマット機能を、デバイスが実際に満たしているか確認する
        // (R16_UNORM等の16bit正規化形式は任意機能のため、非対応なら明確なエラーにする)。
        let supported = unsafe {
            self.instance
                .instance
                .get_physical_device_format_properties(self.physical_device, format)
        }
        .optimal_tiling_features;
        for (usage_flag, feature, name) in [
            (
                vk::ImageUsageFlags::SAMPLED,
                vk::FormatFeatureFlags::SAMPLED_IMAGE,
                "sampling",
            ),
            (
                vk::ImageUsageFlags::STORAGE,
                vk::FormatFeatureFlags::STORAGE_IMAGE,
                "storage image",
            ),
            (
                vk::ImageUsageFlags::COLOR_ATTACHMENT,
                vk::FormatFeatureFlags::COLOR_ATTACHMENT,
                "color attachment",
            ),
        ] {
            if usage.contains(usage_flag) && !supported.contains(feature) {
                anyhow::bail!("This device does not support {format:?} for {name} usage");
            }
        }

        let image_create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let handle = unsafe { self.device.create_image(&image_create_info, None) }
            .context("Failed to create Vulkan image")?;
        let requirements = unsafe { self.device.get_image_memory_requirements(handle) };

        let allocation = self
            .allocator
            .allocate(&AllocationCreateDesc {
                name,
                requirements,
                location: MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .inspect_err(|_| unsafe { self.device.destroy_image(handle, None) })?;

        unsafe {
            self.device
                .bind_image_memory(handle, allocation.memory(), allocation.offset())
        }
        .context("Failed to bind image memory")?;

        Ok(Arc::new(Texture {
            device: self.clone(),
            handle,
            allocation: Some(allocation),
            imported_memory: None,
            format,
            extent,
            layout: std::sync::Mutex::new(vk::ImageLayout::UNDEFINED),
        }))
    }

    pub(crate) fn create_vk_texture_view(
        self: &Arc<Self>,
        texture: &Arc<Texture>,
        aspect_mask: vk::ImageAspectFlags,
    ) -> Result<Arc<TextureView>> {
        let view_create_info = vk::ImageViewCreateInfo::default()
            .image(texture.handle())
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(texture.vk_format())
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let handle = unsafe { self.device.create_image_view(&view_create_info, None) }
            .context("Failed to create Vulkan image view")?;

        Ok(Arc::new(TextureView {
            device: self.clone(),
            handle,
            texture: texture.clone(),
        }))
    }

    pub(crate) fn create_vk_sampler(
        self: &Arc<Self>,
        address_mode: vk::SamplerAddressMode,
        filter: vk::Filter,
    ) -> Result<Arc<Sampler>> {
        let create_info = vk::SamplerCreateInfo::default()
            .mag_filter(filter)
            .min_filter(filter)
            .address_mode_u(address_mode)
            .address_mode_v(address_mode)
            .address_mode_w(address_mode);

        let handle = unsafe { self.device.create_sampler(&create_info, None) }
            .context("Failed to create Vulkan sampler")?;

        Ok(Arc::new(Sampler {
            device: self.clone(),
            handle,
        }))
    }
}

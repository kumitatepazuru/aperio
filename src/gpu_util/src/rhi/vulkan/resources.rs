// Buffer/Texture/TextureView/Sampler の生成と、それに対応するVulkanリソース
// (vkBuffer/vkImage/vkImageView/vkSampler + gpu-allocatorのAllocation)の
// 解放をDropで自動化する所有権付きラッパー。

use std::sync::Arc;

use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan::{AllocationCreateDesc, AllocationScheme};
use gpu_allocator::MemoryLocation;

use super::device::VulkanDevice;

struct BufferInner {
    device: Arc<VulkanDevice>,
    handle: vk::Buffer,
    allocation: Option<gpu_allocator::vulkan::Allocation>,
}

impl Drop for BufferInner {
    fn drop(&mut self) {
        unsafe { self.device.device.destroy_buffer(self.handle, None) };
        if let Some(allocation) = self.allocation.take() {
            let _ = self.device.allocator.free(allocation);
        }
    }
}

#[derive(Clone)]
pub struct Buffer(Arc<BufferInner>);

impl Buffer {
    pub fn handle(&self) -> vk::Buffer {
        self.0.handle
    }

    /// CPUマップされている場合、そのポインタを返す(`location`が
    /// `CpuToGpu`/`GpuToCpu`で確保した場合のみ`Some`になりうる)。
    pub fn mapped_ptr(&self) -> Option<std::ptr::NonNull<std::ffi::c_void>> {
        self.0.allocation.as_ref().and_then(|a| a.mapped_ptr())
    }
}

struct TextureInner {
    device: Arc<VulkanDevice>,
    handle: vk::Image,
    allocation: Option<gpu_allocator::vulkan::Allocation>,
    format: vk::Format,
    extent: vk::Extent3D,
}

impl Drop for TextureInner {
    fn drop(&mut self) {
        unsafe { self.device.device.destroy_image(self.handle, None) };
        if let Some(allocation) = self.allocation.take() {
            let _ = self.device.allocator.free(allocation);
        }
    }
}

#[derive(Clone)]
pub struct Texture(Arc<TextureInner>);

impl Texture {
    pub fn handle(&self) -> vk::Image {
        self.0.handle
    }

    pub fn format(&self) -> vk::Format {
        self.0.format
    }

    pub fn extent(&self) -> vk::Extent3D {
        self.0.extent
    }
}

struct TextureViewInner {
    device: Arc<VulkanDevice>,
    handle: vk::ImageView,
    /// ビューが参照するイメージが生きている間はビューも有効でなければならない
    /// ため、所有権を保持しておく(vkImageViewはvkImageより先に破棄する必要がある)。
    _texture: Texture,
}

impl Drop for TextureViewInner {
    fn drop(&mut self) {
        unsafe { self.device.device.destroy_image_view(self.handle, None) };
    }
}

#[derive(Clone)]
pub struct TextureView(Arc<TextureViewInner>);

impl TextureView {
    pub fn handle(&self) -> vk::ImageView {
        self.0.handle
    }
}

struct SamplerInner {
    device: Arc<VulkanDevice>,
    handle: vk::Sampler,
}

impl Drop for SamplerInner {
    fn drop(&mut self) {
        unsafe { self.device.device.destroy_sampler(self.handle, None) };
    }
}

#[derive(Clone)]
pub struct Sampler(Arc<SamplerInner>);

impl Sampler {
    pub fn handle(&self) -> vk::Sampler {
        self.0.handle
    }
}

impl VulkanDevice {
    /// `usage`を満たすバッファを1つ確保する。
    /// locationはCPU/GPUどちらからアクセスするかのヒント(gpu_allocator::MemoryLocation)。
    pub fn create_buffer(
        self: &Arc<Self>,
        size: u64,
        usage: vk::BufferUsageFlags,
        location: MemoryLocation,
        name: &str,
    ) -> Result<Buffer> {
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

        Ok(Buffer(Arc::new(BufferInner {
            device: self.clone(),
            handle,
            allocation: Some(allocation),
        })))
    }

    /// 2Dテクスチャを1枚確保する。
    pub fn create_texture(
        self: &Arc<Self>,
        width: u32,
        height: u32,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        name: &str,
    ) -> Result<Texture> {
        let extent = vk::Extent3D {
            width,
            height,
            depth: 1,
        };

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

        Ok(Texture(Arc::new(TextureInner {
            device: self.clone(),
            handle,
            allocation: Some(allocation),
            format,
            extent,
        })))
    }

    pub fn create_texture_view(
        self: &Arc<Self>,
        texture: &Texture,
        aspect_mask: vk::ImageAspectFlags,
    ) -> Result<TextureView> {
        let view_create_info = vk::ImageViewCreateInfo::default()
            .image(texture.handle())
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(texture.format())
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let handle = unsafe { self.device.create_image_view(&view_create_info, None) }
            .context("Failed to create Vulkan image view")?;

        Ok(TextureView(Arc::new(TextureViewInner {
            device: self.clone(),
            handle,
            _texture: texture.clone(),
        })))
    }

    pub fn create_sampler(
        self: &Arc<Self>,
        address_mode: vk::SamplerAddressMode,
        filter: vk::Filter,
    ) -> Result<Sampler> {
        let create_info = vk::SamplerCreateInfo::default()
            .mag_filter(filter)
            .min_filter(filter)
            .address_mode_u(address_mode)
            .address_mode_v(address_mode)
            .address_mode_w(address_mode);

        let handle = unsafe { self.device.create_sampler(&create_info, None) }
            .context("Failed to create Vulkan sampler")?;

        Ok(Sampler(Arc::new(SamplerInner {
            device: self.clone(),
            handle,
        })))
    }
}

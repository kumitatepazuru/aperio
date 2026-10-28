// gpu-allocator (純Rust, ash-native) を使った汎用サブアロケーション。
// 外部メモリインポート(dmabuf/D3D12 NTハンドル)されたテクスチャはこのアロケータを経由せず、専用の vkAllocateMemory を直接使う

use std::sync::Mutex;

use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, Allocator, AllocatorCreateDesc};

use super::instance::VulkanInstance;

pub struct VulkanAllocator {
    inner: Mutex<Allocator>,
}

impl VulkanAllocator {
    pub fn new(
        instance: &VulkanInstance,
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
    ) -> Result<Self> {
        let allocator = Allocator::new(&AllocatorCreateDesc {
            instance: instance.instance.clone(),
            device: device.clone(),
            physical_device,
            debug_settings: Default::default(),
            buffer_device_address: false,
            allocation_sizes: Default::default(),
        })
        .context("Failed to create gpu-allocator Vulkan allocator")?;

        Ok(Self {
            inner: Mutex::new(allocator),
        })
    }

    /// CpuToGpu / GpuToCpuのメモリはgpu-allocatorが常にHOST_COHERENTを要求するため、
    /// マップ済みポインタ経由のCPU読み書きにflush/invalidateは不要。
    pub fn allocate(&self, desc: &AllocationCreateDesc<'_>) -> Result<Allocation> {
        self.inner
            .lock()
            .unwrap()
            .allocate(desc)
            .context("gpu-allocator allocation failed")
    }

    pub fn free(&self, allocation: Allocation) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .free(allocation)
            .context("gpu-allocator free failed")
    }
}

// VulkanDeviceがgpu-allocator経由でCPUから書き込み可能なバッファを実際に
// 確保でき、マップされたポインタへ安全に読み書きできることを確認する。

use ash::vk;
use gpu_allocator::MemoryLocation;
use gpu_util::rhi::vulkan::VulkanDevice;

#[test]
fn allocates_and_writes_host_visible_buffer() {
    let device = match VulkanDevice::new() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Skipping allocates_and_writes_host_visible_buffer: no Vulkan device ({e:#})");
            return;
        }
    };

    let size = 256u64;
    let buffer = device
        .create_buffer(
            size,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            MemoryLocation::CpuToGpu,
            "test host-visible buffer",
        )
        .expect("buffer allocation should succeed");

    let ptr = buffer
        .mapped_ptr()
        .expect("CpuToGpu allocation should be host-mapped")
        .cast::<u8>();

    unsafe {
        for i in 0..size as usize {
            ptr.as_ptr().add(i).write(i as u8);
        }
        for i in 0..size as usize {
            assert_eq!(ptr.as_ptr().add(i).read(), i as u8);
        }
    }
}

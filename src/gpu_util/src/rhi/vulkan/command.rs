// コマンドプール/バッファの生成と、単純な同期実行(記録→submit→フェンス待ち)ヘルパー。

use anyhow::{Context, Result};
use ash::vk;

use super::device::VulkanDevice;

impl VulkanDevice {
    pub(crate) fn create_command_pool(&self) -> Result<vk::CommandPool> {
        let create_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(self.queue_family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        unsafe { self.device.create_command_pool(&create_info, None) }
            .context("Failed to create Vulkan command pool")
    }

    pub(crate) fn allocate_command_buffer(&self, pool: vk::CommandPool) -> Result<vk::CommandBuffer> {
        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let buffers = unsafe { self.device.allocate_command_buffers(&alloc_info) }
            .context("Failed to allocate Vulkan command buffer")?;
        Ok(buffers[0])
    }

    /// recordでコマンドバッファに記録し、キューへsubmitして完了までブロックする単純な同期実行ヘルパー。
    pub(crate) fn submit_and_wait(
        &self,
        command_buffer: vk::CommandBuffer,
        record: impl FnOnce(vk::CommandBuffer),
    ) -> Result<()> {
        let begin_info =
            vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe { self.device.begin_command_buffer(command_buffer, &begin_info) }
            .context("Failed to begin command buffer")?;

        record(command_buffer);

        unsafe { self.device.end_command_buffer(command_buffer) }
            .context("Failed to end command buffer")?;

        let fence = unsafe { self.device.create_fence(&vk::FenceCreateInfo::default(), None) }
            .context("Failed to create fence")?;

        let command_buffers = [command_buffer];
        let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);

        let queue = self.queue();
        let submit_result = {
            let raw_queue = queue.raw.lock().unwrap();
            unsafe { self.device.queue_submit(*raw_queue, &[submit_info], fence) }
        };

        if let Err(e) = submit_result {
            unsafe { self.device.destroy_fence(fence, None) };
            return Err(e).context("Failed to submit command buffer");
        }

        let wait_result = unsafe { self.device.wait_for_fences(&[fence], true, u64::MAX) };
        unsafe { self.device.destroy_fence(fence, None) };
        wait_result.context("Failed to wait for fence")?;

        Ok(())
    }
}

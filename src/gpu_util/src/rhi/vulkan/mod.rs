// rhiのVulkan(ash)バックエンド。型はすべてrhiのenumの内側からのみ使われる。

mod command;
mod descriptor;
mod device;
mod dispatch;
mod instance;
mod memory;
mod pipeline;
mod resources;

pub use device::VulkanDevice;
pub use dispatch::DispatchOutput;
pub use pipeline::ComputePipeline;
pub use resources::{Buffer, Sampler, Texture, TextureView};

use std::sync::Mutex;

use ash::vk;

/// サブミット順序を守るため、生のvk::QueueをMutexで保護する。
pub struct Queue {
    pub(crate) raw: Mutex<vk::Queue>,
    #[allow(dead_code)]
    pub(crate) family_index: u32,
}

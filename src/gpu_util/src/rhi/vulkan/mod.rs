// rhiのVulkan(ash)バックエンド。型はすべてrhiのenumの内側からのみ使われる。

mod bindings;
mod command;
mod descriptor;
mod device;
mod dispatch;
mod graphics;
mod instance;
mod memory;
mod native_texture;
mod pipeline;
mod resources;

pub use bindings::{Resource, ResourceBinding};
pub use device::VulkanDevice;
pub use dispatch::TextureCopy;
pub use graphics::{Draw, GraphicsPipeline};
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

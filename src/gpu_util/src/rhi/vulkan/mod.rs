// `rhi::Backend`のVulkan(ash)実装。素のVulkanハンドルを薄くラップするのみ

pub mod device;
pub mod instance;
pub mod memory;
pub mod resources;

pub use device::VulkanDevice;
pub use instance::VulkanInstance;
pub use resources::{Buffer, Sampler, Texture, TextureView};

use crate::rhi::Backend;
use ash::vk;
use std::sync::Mutex;

pub struct ComputePipeline {
    pub handle: vk::Pipeline,
    pub layout: vk::PipelineLayout,
}

pub struct RenderPipeline {
    pub handle: vk::Pipeline,
    pub layout: vk::PipelineLayout,
}

pub struct DescriptorSet {
    pub handle: vk::DescriptorSet,
}

pub struct CommandEncoder {
    pub buffer: vk::CommandBuffer,
}

pub struct Fence {
    pub handle: vk::Fence,
}

/// サブミット順序を守るため、生の`vk::Queue`を`Mutex`で保護する
/// (ashの生キューハンドルはそれ自体スレッドセーフではない)。
pub struct Queue {
    pub(crate) raw: Mutex<vk::Queue>,
    #[allow(dead_code)]
    pub(crate) family_index: u32,
}

/// `rhi::Backend`のVulkan実装であることを示すマーカー型。
pub struct VulkanBackend;

impl Backend for VulkanBackend {
    type Device = std::sync::Arc<VulkanDevice>;
    type Queue = Queue;
    type Buffer = Buffer;
    type Texture = Texture;
    type TextureView = TextureView;
    type Sampler = Sampler;
    type ComputePipeline = ComputePipeline;
    type RenderPipeline = RenderPipeline;
    type DescriptorSet = DescriptorSet;
    type CommandEncoder = CommandEncoder;
    type Fence = Fence;
}

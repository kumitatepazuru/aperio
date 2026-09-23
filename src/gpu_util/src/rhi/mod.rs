// バックエンド非依存の GPU RHI インターフェース。`rhi::Backend`の具象実装は各バックエンド側で定義する。
pub mod vulkan;

/// 入力テクスチャの束ね方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputTextureLayout {
    /// コンパイル時に本数が決まっている固定長の入力テクスチャ配列。
    Fixed(u32),
    /// 実行時に本数が決まる可変長の入力テクスチャ配列
    /// (`VK_DESCRIPTOR_BINDING_VARIABLE_DESCRIPTOR_COUNT_BIT`で宣言する)。
    /// `max_count`はディスクリプタプール確保時に使う上限。
    Variable { max_count: u32 },
}

/// バックエンドが提供する型一式。各フィールドは対応する具象バックエンド
/// (例: `vulkan::VulkanBackend`)がそれぞれの型を割り当てる。
pub trait Backend: Sized {
    type Device;
    type Queue;
    type Buffer: Clone;
    type Texture: Clone;
    type TextureView: Clone;
    type Sampler: Clone;
    type ComputePipeline;
    type RenderPipeline;
    type DescriptorSet;
    type CommandEncoder;
    type Fence;
}

/// このクレートが実際に使うバックエンド。Windows/Mac/Linuxとも Vulkan 固定。
pub type ActiveBackend = vulkan::VulkanBackend;

pub type Device = <ActiveBackend as Backend>::Device;
pub type Queue = <ActiveBackend as Backend>::Queue;

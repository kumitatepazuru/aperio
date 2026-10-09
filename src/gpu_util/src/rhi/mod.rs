// バックエンド非依存のGPU RHI。各リソース型はバックエンドごとのvariantを持つ
// enumで、メソッドはmatchでバックエンド実装へ転送する。

mod vulkan;

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;

use anyhow::{bail, Result};

use crate::image_pixel_format::ImagePixelFormat;

/// 入力テクスチャの束ね方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputArity {
    /// 固定長入力。実際の本数とディスパッチ時の入力テクスチャ数は一致する必要がある。
    Fixed,
    /// Texture2D<float4> tex[]のような単一の可変長配列バインディング。
    Variable,
}

macro_rules! bitflags_u8 {
    ($(#[$meta:meta])* $name:ident { $($variant:ident = $bit:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
        pub struct $name(u8);
        impl $name {
            $(pub const $variant: Self = Self(1 << $bit);)+

            pub const fn empty() -> Self { Self(0) }
            pub const fn contains(self, other: Self) -> bool { self.0 & other.0 == other.0 }
        }
        impl std::ops::BitOr for $name {
            type Output = Self;
            fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
        }
    };
}

bitflags_u8!(
    /// テクスチャの用途。
    TextureUsage {
        SAMPLED = 0,
        STORAGE = 1,
        TRANSFER_SRC = 2,
        TRANSFER_DST = 3,
    }
);

bitflags_u8!(
    /// バッファの用途。
    BufferUsage {
        STORAGE = 0,
        TRANSFER_SRC = 1,
        TRANSFER_DST = 2,
    }
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressMode {
    ClampToEdge,
    Repeat,
    MirrorRepeat,
    ClampToBorder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    Nearest,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SamplerOptions {
    pub address_mode: AddressMode,
    pub filter: FilterMode,
}

/// コンピュートパイプラインの出力リソースの種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    Texture,
    Buffer,
}

/// パイプライン生成時に指定する、シェーダーのリソース形状。
///
/// 入力=set 0、`res`=set 1で、出力・サンプラー・paramsがこの順にbinding 0,1,2...
/// に従うシェーダーを前提とする。
#[derive(Debug, Clone)]
pub struct PipelineDesc {
    pub entry_point: String,
    pub input_arity: InputArity,
    /// input_arity == Fixedの場合の入力本数。Variableでは無視される。
    pub input_count: u32,
    pub has_sampler: bool,
    pub has_params: bool,
    pub output: OutputKind,
}

#[derive(Clone)]
pub enum Buffer {
    Vulkan(Arc<vulkan::Buffer>),
}

#[derive(Clone, Debug)]
pub enum Texture {
    Vulkan(Arc<vulkan::Texture>),
}

#[derive(Clone)]
pub enum TextureView {
    Vulkan(Arc<vulkan::TextureView>),
}

#[derive(Clone)]
pub enum Sampler {
    Vulkan(Arc<vulkan::Sampler>),
}

#[derive(Clone)]
pub enum ComputePipeline {
    Vulkan(Arc<vulkan::ComputePipeline>),
}

impl Buffer {
    /// CPUマップされている場合、そのポインタを返す。
    pub fn mapped_ptr(&self) -> Option<NonNull<c_void>> {
        match self {
            Self::Vulkan(b) => b.mapped_ptr(),
        }
    }
}

impl Texture {
    pub fn width(&self) -> u32 {
        match self {
            Self::Vulkan(t) => t.width(),
        }
    }

    pub fn height(&self) -> u32 {
        match self {
            Self::Vulkan(t) => t.height(),
        }
    }

    pub fn format(&self) -> ImagePixelFormat {
        match self {
            Self::Vulkan(t) => t.format(),
        }
    }
}

impl ComputePipeline {
    pub fn desc(&self) -> &PipelineDesc {
        match self {
            Self::Vulkan(p) => p.desc(),
        }
    }
}

/// コンピュートディスパッチの出力先。
pub enum DispatchOutput<'a> {
    Texture(&'a TextureView),
    Buffer(&'a Buffer),
}

/// 1回のコンピュートディスパッチに必要な情報。形状はPipelineDescと一致させる。
pub struct ComputeDispatch<'a> {
    pub pipeline: &'a ComputePipeline,
    pub inputs: &'a [TextureView],
    pub output: DispatchOutput<'a>,
    pub sampler: Option<&'a Sampler>,
    pub params: Option<&'a Buffer>,
    pub workgroups: (u32, u32, u32),
}

#[derive(Clone)]
pub enum Device {
    Vulkan(Arc<vulkan::VulkanDevice>),
}

impl Device {
    /// 利用可能なバックエンドを順に試して生成する(現状はVulkanのみ)。
    pub fn new() -> Result<Self> {
        match vulkan::VulkanDevice::new() {
            Ok(device) => Ok(Self::Vulkan(device)),
            Err(e) => bail!("Failed to initialize any GPU RHI backend. Vulkan: {e:#}"),
        }
    }

    pub fn create_buffer(
        &self,
        size: u64,
        usage: BufferUsage,
        cpu_visible: bool,
        label: &str,
    ) -> Result<Buffer> {
        match self {
            Self::Vulkan(d) => Ok(Buffer::Vulkan(d.create_buffer(size, usage, cpu_visible, label)?)),
        }
    }

    pub fn create_texture(
        &self,
        width: u32,
        height: u32,
        format: ImagePixelFormat,
        usage: TextureUsage,
        label: &str,
    ) -> Result<Texture> {
        match self {
            Self::Vulkan(d) => Ok(Texture::Vulkan(
                d.create_texture(width, height, format, usage, label)?,
            )),
        }
    }

    pub fn create_texture_view(&self, texture: &Texture) -> Result<TextureView> {
        match (self, texture) {
            (Self::Vulkan(d), Texture::Vulkan(t)) => Ok(TextureView::Vulkan(d.create_texture_view(t)?)),
        }
    }

    pub fn create_sampler(&self, options: &SamplerOptions) -> Result<Sampler> {
        match self {
            Self::Vulkan(d) => Ok(Sampler::Vulkan(d.create_sampler(options)?)),
        }
    }

    pub fn create_compute_pipeline(&self, spirv: &[u32], desc: &PipelineDesc) -> Result<ComputePipeline> {
        match self {
            Self::Vulkan(d) => Ok(ComputePipeline::Vulkan(d.create_compute_pipeline(spirv, desc)?)),
        }
    }

    /// ディスクリプタ構築・コマンド記録・submit・完了待ちをまとめて同期実行する。
    pub fn dispatch_compute(&self, dispatch: ComputeDispatch<'_>) -> Result<()> {
        match self {
            Self::Vulkan(d) => {
                let ComputeDispatch { pipeline, inputs, output, sampler, params, workgroups } = dispatch;
                let ComputePipeline::Vulkan(pipeline) = pipeline;
                let inputs: Vec<&vulkan::TextureView> = inputs
                    .iter()
                    .map(|v| match v {
                        TextureView::Vulkan(v) => &**v,
                    })
                    .collect();
                let output = match output {
                    DispatchOutput::Texture(TextureView::Vulkan(v)) => vulkan::DispatchOutput::Texture(v),
                    DispatchOutput::Buffer(Buffer::Vulkan(b)) => vulkan::DispatchOutput::Buffer(b),
                };
                let sampler = sampler.map(|Sampler::Vulkan(s)| &**s);
                let params = params.map(|Buffer::Vulkan(b)| &**b);
                d.dispatch_compute(pipeline, &inputs, output, sampler, params, workgroups)
            }
        }
    }

    /// CPU側のバイト列をテクスチャへアップロードする。
    pub fn upload_texture_data(&self, texture: &Texture, data: &[u8]) -> Result<()> {
        match (self, texture) {
            (Self::Vulkan(d), Texture::Vulkan(t)) => d.upload_texture_data(t, data),
        }
    }

    /// テクスチャの内容をCPU側へダウンロードする(生バイト列)。
    pub fn download_texture_data(&self, texture: &Texture) -> Result<Vec<u8>> {
        match (self, texture) {
            (Self::Vulkan(d), Texture::Vulkan(t)) => d.download_texture_data(t),
        }
    }

    /// 可変長入力テクスチャ配列に使える最大本数。
    pub fn max_variable_texture_array_len(&self) -> u32 {
        match self {
            Self::Vulkan(d) => d.max_variable_texture_array_len(),
        }
    }

    /// 確保できる2Dテクスチャの最大辺長(px)。
    pub fn maximum_texture_size(&self) -> u32 {
        match self {
            Self::Vulkan(d) => d.maximum_texture_size(),
        }
    }
}

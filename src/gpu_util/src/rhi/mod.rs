// バックエンド非依存のGPU RHI。各リソース型はバックエンドごとのvariantを持つ
// enumで、メソッドはmatchでバックエンド実装へ転送する。

mod vulkan;

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;

use anyhow::{bail, Result};

use crate::image_pixel_format::ImagePixelFormat;

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
        // グラフィックスパイプラインのカラーターゲットとして描画する。
        COLOR_TARGET = 4,
    }
);

bitflags_u8!(
    /// バッファの用途。
    BufferUsage {
        STORAGE = 0,
        TRANSFER_SRC = 1,
        TRANSFER_DST = 2,
        UNIFORM = 3,
        VERTEX = 4,
    }
);

bitflags_u8!(
    /// リソースを参照するシェーダーステージ。
    ShaderStages {
        COMPUTE = 0,
        VERTEX = 1,
        FRAGMENT = 2,
    }
);

/// テクスチャのピクセル形式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureFormat {
    Rgba8Unorm,
    Bgra8Unorm,
    Rgba16Float,
    Rgba32Float,
    R8Unorm,
    Rg8Unorm,
    /// 16bit正規化形式は任意機能のため、非対応のデバイスではテクスチャ生成が失敗する。
    R16Unorm,
    Rg16Unorm,
}

impl TextureFormat {
    pub fn bytes_per_texel(self) -> u32 {
        match self {
            Self::R8Unorm => 1,
            Self::Rg8Unorm | Self::R16Unorm => 2,
            Self::Rgba8Unorm | Self::Bgra8Unorm | Self::Rg16Unorm => 4,
            Self::Rgba16Float => 8,
            Self::Rgba32Float => 16,
        }
    }
}

impl From<ImagePixelFormat> for TextureFormat {
    fn from(format: ImagePixelFormat) -> Self {
        match format {
            ImagePixelFormat::Rgba8Unorm => Self::Rgba8Unorm,
            ImagePixelFormat::Rgba16Float => Self::Rgba16Float,
            ImagePixelFormat::Rgba32Float => Self::Rgba32Float,
        }
    }
}

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

/// ディスクリプタ1つのリソース種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingKind {
    SampledImage,
    StorageImage,
    StorageBuffer,
    UniformBuffer,
    Sampler,
}

/// セット内の1バインディングの記述。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BindingDesc {
    pub binding: u32,
    pub kind: BindingKind,
    /// 配列の本数(配列でないなら1)。`variable_count`がtrueの場合は無視され、
    /// デバイスの上限が最大本数として使われる(実際の本数はディスパッチ時の
    /// `Resource::TextureArray`の長さで決まる)。
    pub count: u32,
    /// 実行時に本数が決まる可変長配列。`SampledImage`のみ指定でき、
    /// セット内で最大のbinding番号に置く必要がある。
    pub variable_count: bool,
    pub stages: ShaderStages,
}

/// 1つのディスクリプタセットの形。
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct SetLayoutDesc {
    pub bindings: Vec<BindingDesc>,
}

/// パイプライン全体のリソースレイアウト(セット番号は`sets`の添字)。
/// RHIはここで渡された形をそのまま処理するだけで、特定の規約は持たない。
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct PipelineLayoutDesc {
    pub sets: Vec<SetLayoutDesc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertexFormat {
    Float32x2,
    Float32x3,
    Float32x4,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VertexAttribute {
    pub location: u32,
    pub format: VertexFormat,
    pub offset: u32,
}

/// 頂点バッファ1本ぶんの並び。`GraphicsPipelineDesc::vertex_buffers`の添字が
/// 頂点バッファのスロット番号になる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VertexBufferLayout {
    pub stride: u32,
    /// trueならインスタンスごと、falseなら頂点ごとに進める。
    pub per_instance: bool,
    pub attributes: Vec<VertexAttribute>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendFactor {
    Zero,
    One,
    SrcAlpha,
    OneMinusSrcAlpha,
}

/// ブレンド演算は常にAdd。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlendComponent {
    pub src_factor: BlendFactor,
    pub dst_factor: BlendFactor,
}

/// 色とアルファを別々に指定できるブレンド設定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlendState {
    pub color: BlendComponent,
    pub alpha: BlendComponent,
}

/// グラフィックスパイプライン(三角形リスト、カラーターゲット1枚、深度なし)の記述。
pub struct GraphicsPipelineDesc<'a> {
    pub vertex_spirv: &'a [u32],
    pub vertex_entry: &'a str,
    pub fragment_spirv: &'a [u32],
    pub fragment_entry: &'a str,
    pub layout: PipelineLayoutDesc,
    pub vertex_buffers: Vec<VertexBufferLayout>,
    pub color_format: TextureFormat,
    /// Noneならブレンドなし(上書き)。
    pub blend: Option<BlendState>,
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

#[derive(Clone)]
pub enum GraphicsPipeline {
    Vulkan(Arc<vulkan::GraphicsPipeline>),
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

    pub fn format(&self) -> TextureFormat {
        match self {
            Self::Vulkan(t) => t.format(),
        }
    }
}

impl ComputePipeline {
    pub fn layout(&self) -> &PipelineLayoutDesc {
        match self {
            Self::Vulkan(p) => p.layout_desc(),
        }
    }
}

/// 1バインディングに束縛するリソース。
#[derive(Clone, Copy)]
pub enum Resource<'a> {
    Texture(&'a TextureView),
    /// 配列バインディング(固定長なら`count`と、可変長ならデバイス上限以下の本数)。
    TextureArray(&'a [TextureView]),
    Buffer(&'a Buffer),
    Sampler(&'a Sampler),
}

pub struct ResourceBinding<'a> {
    pub binding: u32,
    pub resource: Resource<'a>,
}

/// 1回のコンピュートディスパッチに必要な情報。
/// `sets[i]`がパイプラインレイアウトのセットiに対応し、各セットのバインディングは
/// すべて過不足なく指定する必要がある。
pub struct ComputeDispatch<'a> {
    pub pipeline: &'a ComputePipeline,
    pub sets: &'a [&'a [ResourceBinding<'a>]],
    pub workgroups: (u32, u32, u32),
}

impl<'a> ResourceBinding<'a> {
    fn to_vulkan(&self) -> vulkan::ResourceBinding<'a> {
        let resource = match self.resource {
            Resource::Texture(TextureView::Vulkan(v)) => vulkan::Resource::Texture(v),
            Resource::TextureArray(views) => vulkan::Resource::TextureArray(
                views
                    .iter()
                    .map(|TextureView::Vulkan(v)| &**v)
                    .collect(),
            ),
            Resource::Buffer(Buffer::Vulkan(b)) => vulkan::Resource::Buffer(b),
            Resource::Sampler(Sampler::Vulkan(s)) => vulkan::Resource::Sampler(s),
        };
        vulkan::ResourceBinding {
            binding: self.binding,
            resource,
        }
    }
}

/// 他のグラフィックスAPI(D3D12 / dmabuf)が所有するテクスチャへのハンドル。
/// `Device::import_external_texture`でRHIのテクスチャとして取り込める。
#[derive(Debug, Clone)]
pub enum ExternalTextureHandle {
    /// Windows: D3D12_HEAP_FLAG_SHARED付きで作られたリソースの`CreateSharedHandle`のNTハンドル。
    /// ハンドルの所有権は呼び出し側に残る。
    #[cfg(target_os = "windows")]
    D3D12Resource { nt_handle: usize },
    /// Linux: dmabuf。fdは複製してから取り込むので、呼び出し側のfdはそのまま残る。
    /// 単一プレーンのみ対応。`modifier`はDRM format modifier(0=LINEAR)。
    #[cfg(target_os = "linux")]
    DmaBuf { planes: Vec<DmaBufPlane>, modifier: u64 },
}

/// dmabufの1プレーン分の情報。
#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
pub struct DmaBufPlane {
    pub fd: std::os::fd::RawFd,
    pub stride: u32,
    pub offset: u32,
    pub size: u32,
}

/// `Device::render_pass`の1回の描画コマンド。
/// `sets`はパイプラインレイアウトのセットに対応する(`ComputeDispatch`と同じ規則)。
pub struct Draw<'a> {
    pub pipeline: &'a GraphicsPipeline,
    pub sets: &'a [&'a [ResourceBinding<'a>]],
    /// スロット番号順の頂点バッファ。
    pub vertex_buffers: &'a [&'a Buffer],
    pub vertex_count: u32,
    pub instance_count: u32,
}

/// `Device::copy_textures`の1件分(フォーマットが一致する矩形コピー)。
pub struct TextureCopy<'a> {
    pub src: &'a Texture,
    pub src_origin: (u32, u32),
    pub dst: &'a Texture,
    pub dst_origin: (u32, u32),
    pub size: (u32, u32),
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
        format: TextureFormat,
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

    pub fn create_compute_pipeline(
        &self,
        spirv: &[u32],
        entry_point: &str,
        layout: &PipelineLayoutDesc,
    ) -> Result<ComputePipeline> {
        match self {
            Self::Vulkan(d) => Ok(ComputePipeline::Vulkan(
                d.create_compute_pipeline(spirv, entry_point, layout)?,
            )),
        }
    }

    /// ディスクリプタ構築・コマンド記録・submit・完了待ちをまとめて同期実行する。
    pub fn dispatch_compute(&self, dispatch: ComputeDispatch<'_>) -> Result<()> {
        match self {
            Self::Vulkan(d) => {
                let ComputeDispatch { pipeline, sets, workgroups } = dispatch;
                let ComputePipeline::Vulkan(pipeline) = pipeline;
                let sets: Vec<Vec<vulkan::ResourceBinding<'_>>> = sets
                    .iter()
                    .map(|set| set.iter().map(ResourceBinding::to_vulkan).collect())
                    .collect();
                d.dispatch_compute(pipeline, &sets, workgroups)
            }
        }
    }

    pub fn create_graphics_pipeline(&self, desc: &GraphicsPipelineDesc<'_>) -> Result<GraphicsPipeline> {
        match self {
            Self::Vulkan(d) => Ok(GraphicsPipeline::Vulkan(d.create_graphics_pipeline(desc)?)),
        }
    }

    /// 1つのカラーターゲットへ`draws`を順に描画する(同期)。`clear`がNoneなら既存内容を保持する。
    /// ターゲットは`COLOR_TARGET`用途で作られ、パイプラインの`color_format`と同じ形式である必要がある。
    /// NDCのy+は画面上方向(wgpu / D3D / Metalと同じ向き)。
    pub fn render_pass(
        &self,
        target: &TextureView,
        clear: Option<[f32; 4]>,
        draws: &[Draw<'_>],
    ) -> Result<()> {
        match self {
            Self::Vulkan(d) => {
                let TextureView::Vulkan(target) = target;
                let draws: Vec<vulkan::Draw<'_>> = draws
                    .iter()
                    .map(|draw| {
                        let GraphicsPipeline::Vulkan(pipeline) = draw.pipeline;
                        vulkan::Draw {
                            pipeline,
                            sets: draw
                                .sets
                                .iter()
                                .map(|set| set.iter().map(ResourceBinding::to_vulkan).collect())
                                .collect(),
                            vertex_buffers: draw
                                .vertex_buffers
                                .iter()
                                .map(|Buffer::Vulkan(b)| &**b)
                                .collect(),
                            vertex_count: draw.vertex_count,
                            instance_count: draw.instance_count,
                        }
                    })
                    .collect();
                d.render_pass(target, clear, &draws)
            }
        }
    }

    /// 他のAPIが所有するテクスチャをRHIのテクスチャとして取り込む。
    /// 取り込んだテクスチャは`usage`に従って描画先・コピー先などに使える
    /// (中身は元のリソースと共有されるため、書き込みは外部APIから見える)。
    pub fn import_external_texture(
        &self,
        handle: &ExternalTextureHandle,
        width: u32,
        height: u32,
        format: TextureFormat,
        usage: TextureUsage,
    ) -> Result<Texture> {
        match (self, handle) {
            #[cfg(target_os = "windows")]
            (Self::Vulkan(d), ExternalTextureHandle::D3D12Resource { nt_handle }) => Ok(
                Texture::Vulkan(d.import_d3d12_texture(*nt_handle, width, height, format, usage)?),
            ),
            #[cfg(target_os = "linux")]
            (Self::Vulkan(d), ExternalTextureHandle::DmaBuf { planes, modifier }) => {
                Ok(Texture::Vulkan(d.import_dmabuf_texture(
                    planes, *modifier, width, height, format, usage,
                )?))
            }
        }
    }

    /// このデバイスのアダプタのLUID(外部APIで同じGPUを選ぶために使う)。取得できなければNone。
    pub fn adapter_luid(&self) -> Option<[u8; 8]> {
        match self {
            Self::Vulkan(d) => d.adapter_luid(),
        }
    }

    /// テクスチャの矩形領域へCPU側のバイト列(行詰め)をアップロードする。
    pub fn upload_texture_region(
        &self,
        texture: &Texture,
        origin: (u32, u32),
        size: (u32, u32),
        data: &[u8],
    ) -> Result<()> {
        match (self, texture) {
            (Self::Vulkan(d), Texture::Vulkan(t)) => d.upload_texture_region(t, origin, size, data),
        }
    }

    /// テクスチャ間の矩形コピーを1回のsubmitでまとめて実行する(同期)。
    pub fn copy_textures(&self, copies: &[TextureCopy<'_>]) -> Result<()> {
        match self {
            Self::Vulkan(d) => {
                let copies: Vec<vulkan::TextureCopy<'_>> = copies
                    .iter()
                    .map(|c| {
                        let (Texture::Vulkan(src), Texture::Vulkan(dst)) = (c.src, c.dst);
                        vulkan::TextureCopy {
                            src,
                            src_origin: c.src_origin,
                            dst,
                            dst_origin: c.dst_origin,
                            size: c.size,
                        }
                    })
                    .collect();
                d.copy_textures(&copies)
            }
        }
    }

    /// テクスチャ全体へCPU側のバイト列をアップロードする。
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

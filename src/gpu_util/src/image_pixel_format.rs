// image_pixel_format.rs

/// `ImageGenerator::new()` 構築時にのみ選択できる内部ワーキングテクスチャのフォーマット。
/// 構築後に変更することはできない(パイプライン/テクスチャキャッシュ全体の再生成コストが
/// 大きいため意図的にサポートしない)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImagePixelFormat {
    Rgba8Unorm,
    Rgba16Float,
    Rgba32Float,
}

impl ImagePixelFormat {
    pub fn to_wgpu(self) -> wgpu::TextureFormat {
        match self {
            ImagePixelFormat::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
            ImagePixelFormat::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
            ImagePixelFormat::Rgba32Float => wgpu::TextureFormat::Rgba32Float,
        }
    }
}

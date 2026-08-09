// image_pixel_format.rs

/// `ImageGenerator::new()` 構築時にのみ選択できる内部ワーキングテクスチャのフォーマット。
/// 構築後に変更することはできない(パイプライン/テクスチャキャッシュ全体の再生成コストが
/// 大きいため意図的にサポートしない)。
///
/// 列挙子の宣言順は精度の昇順(`Rgba8Unorm < Rgba16Float < Rgba32Float`)になっており、
/// 導出された`Ord`をそのまま「シェーダーごとの最低要求フォーマットと、ユーザー設定の
/// グローバルフォーマットのうち精度が高い方を選ぶ」ためのフロア合成(`.max()`)に使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

/// `ImageGenerator::new()` 構築時にのみ選択できる内部ワーキングテクスチャのフォーマット。
/// 構築後に変更することはできない
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
    pub fn to_slang_image_format_literal(self) -> &'static str {
        match self {
            ImagePixelFormat::Rgba8Unorm => "\"rgba8\"",
            ImagePixelFormat::Rgba16Float => "\"rgba16f\"",
            ImagePixelFormat::Rgba32Float => "\"rgba32f\"",
        }
    }
}

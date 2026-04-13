use bytemuck::{Pod, Zeroable};

mod glyph_atlas;
pub mod text_renderer;

// ── アトラスのサイズ（px）。2048×2048 で約 16,000 グリフ（12px 時）をカバーする ──
pub const ATLAS_SIZE: u32 = 2048;
pub const CACHE_SIZE: usize = 256;

// ────────────────────────────────────────────────
//  GPU インスタンスデータ（グリフ 1 つ = 1 インスタンス）
// ────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct GlyphInstance {
    pos: [f32; 2],    // 出力テクスチャ上のピクセル左上
    size: [f32; 2],   // グリフのピクセルサイズ
    uv_min: [f32; 2], // アトラス UV 左上（0-1）
    uv_max: [f32; 2], // アトラス UV 右下（0-1）
    color: [f32; 4],  // RGBA（0-1）
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Uniforms {
    output_size: [f32; 2],
    _pad: [f32; 2],
}

// ────────────────────────────────────────────────
//  TextSpec / キャッシュキー
// ────────────────────────────────────────────────

/// テキスト描画の仕様。
pub struct TextSpec {
    pub text: String,
    pub font_size: f32,
    /// RGBA 各 0‥255
    pub color: [u8; 4],
    /// None のときはシステムデフォルトフォント
    pub font_family: Option<String>,
    /// 折り返し最大幅（px）。None のとき折り返しなし
    pub max_width: Option<u32>,
    /// 行間の追加スペース（px）。0.0 のとき行間変更なし
    pub line_spacing: f32,
    /// 文字間の追加スペース（px）。0.0 のとき変更なし
    pub char_spacing: f32,
}

impl Default for TextSpec {
    fn default() -> Self {
        Self {
            text: String::new(),
            font_size: 16.0,
            color: [255, 255, 255, 255],
            font_family: None,
            max_width: None,
            line_spacing: 0.0,
            char_spacing: 0.0,
        }
    }
}

#[derive(Hash, Eq, PartialEq, Clone)]
struct TextCacheKey {
    text: String,
    font_size_bits: u32,
    color: [u8; 4],
    font_family: Option<String>,
    max_width: Option<u32>,
    line_spacing_bits: u32,
    char_spacing_bits: u32,
}

impl TextCacheKey {
    fn from_spec(spec: &TextSpec) -> Self {
        Self {
            text: spec.text.clone(),
            font_size_bits: spec.font_size.to_bits(),
            color: spec.color,
            font_family: spec.font_family.clone(),
            max_width: spec.max_width,
            line_spacing_bits: spec.line_spacing.to_bits(),
            char_spacing_bits: spec.char_spacing.to_bits(),
        }
    }
}

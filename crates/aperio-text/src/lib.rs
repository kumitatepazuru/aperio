use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;

mod glyph_atlas;
pub mod text_renderer;

pub use text_renderer::PreparedText;

// ── アトラスのサイズ（px）。2048×2048 で約 16,000 グリフ（12px 時）をカバーする ──
pub const ATLAS_SIZE: u32 = 2048;

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
//  FontsList
// ────────────────────────────────────────────────

/// フォントファミリー名 → ウェイト値（100/200/…/900）のリスト。
/// `TextRenderer::get_fonts_list` の戻り値型。
pub type FontsList = HashMap<String, Vec<u16>>;

// ────────────────────────────────────────────────
//  TextSpec / CharGlyphData
// ────────────────────────────────────────────────

/// テキスト描画の仕様。
pub struct TextSpec {
    pub text: String,
    pub font_size: f32,
    /// RGBA 各 0.0‥1.0
    pub color: [f32; 4],
    /// None のときはシステムデフォルトフォント
    pub font_family: Option<String>,
    /// フォントウェイト（100=Thin / 400=Regular / 500=Medium / 700=Bold など）。
    /// None のときはシステムデフォルト（通常 400）
    pub font_weight: Option<u16>,
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
            color: [1.0, 1.0, 1.0, 1.0],
            font_family: None,
            font_weight: None,
            max_width: None,
            line_spacing: 0.0,
            char_spacing: 0.0,
        }
    }
}

/// 1 文字分のグリフ位置情報。`run_render_chars` の戻り値要素。
pub struct CharGlyphData {
    pub ch: char,
    /// 全体テクスチャ上の左端 x 座標（px）
    pub x: u32,
    /// 全体テクスチャ上の上端 y 座標（px）
    pub y: u32,
    /// 文字グリフの幅（px）
    pub w: u32,
    /// 文字グリフの高さ（px）
    pub h: u32,
}

/// 動画フレームのカラーフォーマット。
///
/// `get_frame` は常に u8 を返す。
/// `Rgb16Float` / `Rgba16Float` は将来の HDR テクスチャ対応用に予約済み。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorFormat {
    /// RGB 8bit unorm — 3 bytes/pixel
    RgbUnorm,
    /// RGBA 8bit unorm — 4 bytes/pixel
    RgbaUnorm,
    /// RGB 16bit float — 将来用
    Rgb16Float,
    /// RGBA 16bit float — 将来用
    Rgba16Float,
}

impl ColorFormat {
    /// `get_frame` が返す 1 ピクセルあたりのバイト数。
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::RgbUnorm   => 3,
            Self::RgbaUnorm  => 4,
            Self::Rgb16Float  => 6,
            Self::Rgba16Float => 8,
        }
    }

    pub fn has_alpha(self) -> bool {
        matches!(self, Self::RgbaUnorm | Self::Rgba16Float)
    }

    /// `get_frame` 用 GStreamer 出力フォーマット（常に 8bit）。
    pub fn cpu_gst_fmt(self) -> &'static str {
        if self.has_alpha() { "RGBA" } else { "RGB" }
    }

    /// `get_texture_frame` が返すテクスチャのフォーマット（現状は常に Rgba8Unorm）。
    pub fn wgpu_texture_format(self) -> wgpu::TextureFormat {
        wgpu::TextureFormat::Rgba8Unorm
    }
}

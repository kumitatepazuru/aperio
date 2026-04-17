/// GPU 転送時に使うネイティブ動画フォーマット分類。
///
/// 動画ファイルごとに異なる chroma subsampling を変えずにアップロードするため、
/// decodebin が出力するフォーマットをそのまま使う。
#[derive(Debug, Clone)]
pub enum NativeGstFormat {
    /// セミプラナー YUV（NV12, NV21, NV16, NV61 など）
    /// Y プレーン(R8Unorm) + UV インターリーブプレーン(Rg8Unorm)
    SemiPlanar {
        gst_fmt: String,
        chroma_w_div: u32, // 水平 chroma 分母（1=444/422, 2=420）
        chroma_h_div: u32, // 垂直 chroma 分母（1=444/422, 2=420）
        swap_uv: bool,     // true → V が U より先（NV21/NV61）
    },
    /// フルプラナー YUV（I420, YV12, I422, I444 など）
    /// Y/U/V それぞれ独立したプレーン（R8Unorm × 3）
    Planar {
        gst_fmt: String,
        chroma_w_div: u32,
        chroma_h_div: u32,
        swap_uv: bool, // true → plane[1] が V, plane[2] が U（YV12）
    },
    /// YUV でないか複雑な形式 → videoconvert → RGBA 経由で転送
    DirectRgba,
}

impl NativeGstFormat {
    /// GStreamer フォーマット文字列から分類を決定する。
    pub fn from_gst_fmt(fmt: &str) -> Self {
        match fmt {
            // ── セミプラナー ──────────────────────────────────────────
            "NV12" => sp(fmt, 2, 2, false),
            "NV21" => sp(fmt, 2, 2, true),
            "NV16" => sp(fmt, 2, 1, false),
            "NV61" => sp(fmt, 2, 1, true),
            // ── フルプラナー ──────────────────────────────────────────
            "I420" | "IYUV" => pl(fmt, 2, 2, false),
            "YV12"          => pl(fmt, 2, 2, true),
            "I422" | "Y42B" => pl(fmt, 2, 1, false),
            "I444" | "Y444" => pl(fmt, 1, 1, false),
            // ── その他（packed YUV, HDR, RGB 系はすべて DirectRgba）─
            _ => Self::DirectRgba,
        }
    }

    /// GPU パイプラインの capsfilter に使うフォーマット文字列。
    pub fn gpu_pipeline_fmt(&self) -> &str {
        match self {
            Self::SemiPlanar { gst_fmt, .. } => gst_fmt.as_str(),
            Self::Planar { gst_fmt, .. }     => gst_fmt.as_str(),
            Self::DirectRgba                  => "RGBA",
        }
    }

    /// GPU パイプラインに videoconvert が必要か（DirectRgba のみ）。
    pub fn needs_videoconvert(&self) -> bool {
        matches!(self, Self::DirectRgba)
    }
}

// 短縮ヘルパー
fn sp(fmt: &str, w: u32, h: u32, swap: bool) -> NativeGstFormat {
    NativeGstFormat::SemiPlanar {
        gst_fmt: fmt.to_owned(),
        chroma_w_div: w,
        chroma_h_div: h,
        swap_uv: swap,
    }
}
fn pl(fmt: &str, w: u32, h: u32, swap: bool) -> NativeGstFormat {
    NativeGstFormat::Planar {
        gst_fmt: fmt.to_owned(),
        chroma_w_div: w,
        chroma_h_div: h,
        swap_uv: swap,
    }
}

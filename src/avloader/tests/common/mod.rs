// YUV→RGBA変換テストの共通ヘルパー(各テストファイルから`mod common;`で使う)。

#![allow(dead_code)]

use avloader::yuv_pipeline::{PlaneDesc, YuvConvParams, YuvLayout, YuvPipeline};
use gpu_util::image_generator::ImageGenerator;
use gpu_util::rhi::TextureFormat;
use gpu_util::ImagePixelFormat;

pub const WIDTH: u32 = 4;
pub const HEIGHT: u32 = 4;

pub fn generator_or_skip(name: &str) -> Option<ImageGenerator> {
    match ImageGenerator::new(ImagePixelFormat::Rgba16Float) {
        Ok(g) => Some(g),
        Err(e) => {
            eprintln!("Skipping {name}: no GPU ({e:#})");
            None
        }
    }
}

pub fn plane(width: u32, height: u32, format: TextureFormat) -> PlaneDesc {
    PlaneDesc {
        tex_width: width,
        tex_height: height,
        bytes_per_texel: format.bytes_per_texel(),
        format,
    }
}

/// 一様な値で埋めた1プレーン分のバイト列(`texel`は1テクセル分のバイト列)。
pub fn solid(desc: &PlaneDesc, texel: &[u8]) -> Vec<u8> {
    assert_eq!(texel.len() as u32, desc.bytes_per_texel);
    texel.repeat((desc.tex_width * desc.tex_height) as usize)
}

/// `YuvConvParams`と同じ式でBT.709のRGBを求める(8bitの生値を入力)。
pub fn expected_rgb(y: u8, cb: u8, cr: u8, p: &YuvConvParams) -> [f32; 3] {
    let y = (y as f32 / 255.0 - p.y_offset) * p.y_scale;
    let cb = (cb as f32 / 255.0 - p.c_offset) * p.c_scale;
    let cr = (cr as f32 / 255.0 - p.c_offset) * p.c_scale;
    [
        y + 1.5748 * cr,
        y - 0.18732 * cb - 0.46812 * cr,
        y + 1.8556 * cb,
    ]
    .map(|v| v.clamp(0.0, 1.0))
}

/// 変換を実行し、全ピクセルが`expected`(RGBA)に一致することを確認する。
pub fn convert_and_check(
    ig: &ImageGenerator,
    layout: YuvLayout,
    params: YuvConvParams,
    descs: &[PlaneDesc],
    data: &[Vec<u8>],
    expected: [f32; 4],
) {
    let pipeline = YuvPipeline::new(&ig.device, layout, params).expect("pipeline creation");
    let slices: Vec<&[u8]> = data.iter().map(|v| v.as_slice()).collect();
    let out = pipeline
        .convert(ig, descs, &slices, WIDTH, HEIGHT)
        .expect("conversion should succeed");
    let bytes = ig.device.download_texture_data(&out).expect("readback");
    assert_eq!(bytes.len(), (WIDTH * HEIGHT * 8) as usize);
    for (i, px) in bytes.chunks_exact(8).enumerate() {
        for c in 0..4 {
            let got = half::f16::from_le_bytes([px[c * 2], px[c * 2 + 1]]).to_f32();
            assert!(
                (got - expected[c]).abs() < 0.01,
                "pixel {i} channel {c}: expected {}, got {got}",
                expected[c]
            );
        }
    }
}

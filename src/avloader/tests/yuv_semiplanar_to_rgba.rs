// 2プレーン(NV12系、UVインターリーブ)のYUV→RGBA変換。8bitと16bit(P010相当)を確認する。

mod common;

use avloader::yuv_pipeline::{YuvConvParams, YuvLayout};
use common::*;
use gpu_util::rhi::{TextureFormat, TextureUsage};

#[test]
fn converts_nv12_limited_range() {
    let Some(ig) = generator_or_skip("converts_nv12_limited_range") else {
        return;
    };
    let params = YuvConvParams::limited_8bit();
    let y = plane(WIDTH, HEIGHT, TextureFormat::R8Unorm);
    let uv = plane(WIDTH / 2, HEIGHT / 2, TextureFormat::Rg8Unorm);
    let data = vec![solid(&y, &[100]), solid(&uv, &[90, 200])]; // R=Cb, G=Cr
    let [r, g, b] = expected_rgb(100, 90, 200, &params);
    convert_and_check(&ig, YuvLayout::SemiPlanar, params, &[y, uv], &data, [r, g, b, 1.0]);
}

#[test]
fn converts_p010_limited_range() {
    let Some(ig) = generator_or_skip("converts_p010_limited_range") else {
        return;
    };
    // 16bit正規化形式は任意機能。非対応のデバイスではスキップする。
    for format in [TextureFormat::R16Unorm, TextureFormat::Rg16Unorm] {
        if ig
            .device
            .create_texture(1, 1, format, TextureUsage::SAMPLED, "probe")
            .is_err()
        {
            eprintln!("Skipping converts_p010_limited_range: {format:?} unsupported");
            return;
        }
    }

    let params = YuvConvParams::limited_10bit_in_16();
    let y = plane(WIDTH, HEIGHT, TextureFormat::R16Unorm);
    let uv = plane(WIDTH / 2, HEIGHT / 2, TextureFormat::Rg16Unorm);
    // 10bit値を6bit左シフトして16bitに格納する(P010)。
    let word = |v10: u16| (v10 << 6).to_le_bytes();
    let (y10, cb10, cr10) = (400u16, 380u16, 700u16);
    let data = vec![
        solid(&y, &word(y10)),
        solid(&uv, &[word(cb10), word(cr10)].concat()),
    ];

    let norm = |v10: u16| ((v10 << 6) as f32) / 65535.0;
    let yv = (norm(y10) - params.y_offset) * params.y_scale;
    let cb = (norm(cb10) - params.c_offset) * params.c_scale;
    let cr = (norm(cr10) - params.c_offset) * params.c_scale;
    let [r, g, b] = [
        yv + 1.5748 * cr,
        yv - 0.18732 * cb - 0.46812 * cr,
        yv + 1.8556 * cb,
    ]
    .map(|v| v.clamp(0.0, 1.0));

    convert_and_check(&ig, YuvLayout::SemiPlanar, params, &[y, uv], &data, [r, g, b, 1.0]);
}

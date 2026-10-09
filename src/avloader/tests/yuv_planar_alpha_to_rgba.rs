// 4プレーン(YUVA)のYUV→RGBA変換。アルファは独立したプレーンからそのまま出る。

mod common;

use avloader::yuv_pipeline::{YuvConvParams, YuvLayout};
use common::*;
use gpu_util::rhi::TextureFormat;

#[test]
fn converts_planar_alpha_limited_range() {
    let Some(ig) = generator_or_skip("converts_planar_alpha_limited_range") else {
        return;
    };
    let params = YuvConvParams::limited_8bit();
    let y = plane(WIDTH, HEIGHT, TextureFormat::R8Unorm);
    let c = plane(WIDTH / 2, HEIGHT / 2, TextureFormat::R8Unorm);
    let a = plane(WIDTH, HEIGHT, TextureFormat::R8Unorm);
    let data = vec![
        solid(&y, &[100]),
        solid(&c, &[90]),
        solid(&c, &[200]),
        solid(&a, &[128]),
    ];
    let [r, g, b] = expected_rgb(100, 90, 200, &params);
    convert_and_check(
        &ig,
        YuvLayout::PlanarAlpha,
        params,
        &[y, c.clone(), c, a],
        &data,
        [r, g, b, 128.0 / 255.0],
    );
}

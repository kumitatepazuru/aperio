// 3プレーン(I420系)のYUV→RGBA変換。Y=全解像度、Cb/Cr=半解像度。

mod common;

use avloader::yuv_pipeline::{YuvConvParams, YuvLayout};
use common::*;
use gpu_util::rhi::TextureFormat;

#[test]
fn converts_planar_limited_range() {
    let Some(ig) = generator_or_skip("converts_planar_limited_range") else {
        return;
    };
    let params = YuvConvParams::limited_8bit();
    let y = plane(WIDTH, HEIGHT, TextureFormat::R8Unorm);
    let c = plane(WIDTH / 2, HEIGHT / 2, TextureFormat::R8Unorm);
    let data = vec![solid(&y, &[100]), solid(&c, &[90]), solid(&c, &[200])];
    let [r, g, b] = expected_rgb(100, 90, 200, &params);
    convert_and_check(&ig, YuvLayout::Planar, params, &[y, c.clone(), c], &data, [r, g, b, 1.0]);
}

#[test]
fn converts_planar_full_range() {
    let Some(ig) = generator_or_skip("converts_planar_full_range") else {
        return;
    };
    let params = YuvConvParams::full_range_8bit();
    let y = plane(WIDTH, HEIGHT, TextureFormat::R8Unorm);
    let c = plane(WIDTH / 2, HEIGHT / 2, TextureFormat::R8Unorm);
    let data = vec![solid(&y, &[120]), solid(&c, &[110]), solid(&c, &[160])];
    let [r, g, b] = expected_rgb(120, 110, 160, &params);
    convert_and_check(&ig, YuvLayout::Planar, params, &[y, c.clone(), c], &data, [r, g, b, 1.0]);
}

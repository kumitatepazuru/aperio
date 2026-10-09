// 2プレーン(NV21、VUインターリーブ)のYUV→RGBA変換。NV12とはCb/Crの並びだけが逆。

mod common;

use avloader::yuv_pipeline::{YuvConvParams, YuvLayout};
use common::*;
use gpu_util::rhi::TextureFormat;

#[test]
fn converts_nv21_limited_range() {
    let Some(ig) = generator_or_skip("converts_nv21_limited_range") else {
        return;
    };
    let params = YuvConvParams::limited_8bit();
    let y = plane(WIDTH, HEIGHT, TextureFormat::R8Unorm);
    let vu = plane(WIDTH / 2, HEIGHT / 2, TextureFormat::Rg8Unorm);
    let data = vec![solid(&y, &[100]), solid(&vu, &[200, 90])]; // R=Cr, G=Cb
    let [r, g, b] = expected_rgb(100, 90, 200, &params);
    convert_and_check(&ig, YuvLayout::SemiPlanarVU, params, &[y, vu], &data, [r, g, b, 1.0]);
}

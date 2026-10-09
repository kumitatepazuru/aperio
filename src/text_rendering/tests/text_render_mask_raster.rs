// マスクグリフ描画がストレートアルファ(rgbをcoverageで乗算しない)を維持していることの検証。
//
// 赤(1,0,0,1)で大きな文字を描き、アンチエイリアスで被覆率が中間のピクセルでも
// rgbが(1,0,0)のまま、alphaだけが被覆率を表していることを確認する。

use gpu_util::image_generator::ImageGenerator;
use gpu_util::ImagePixelFormat;
use text_rendering::text_renderer::TextRenderer;
use text_rendering::TextSpec;

fn f16_at(bytes: &[u8], index: usize) -> f32 {
    half::f16::from_le_bytes([bytes[index * 2], bytes[index * 2 + 1]]).to_f32()
}

#[test]
fn mask_glyph_keeps_straight_alpha() {
    let image_generator = match ImageGenerator::new(ImagePixelFormat::Rgba16Float) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("Skipping mask_glyph_keeps_straight_alpha: no GPU ({e:#})");
            return;
        }
    };
    let mut renderer = TextRenderer::new(&image_generator).expect("renderer creation");

    let prepared = renderer
        .prepare_render_text(&TextSpec {
            text: "O".to_string(),
            font_size: 64.0,
            color: [1.0, 0.0, 0.0, 1.0],
            ..Default::default()
        })
        .expect("prepare should succeed")
        .expect("'O' should produce a glyph (is a system font available?)");
    let texture = renderer.run_render_text(&prepared).expect("render");
    assert_eq!((texture.width(), texture.height()), (prepared.width, prepared.height));

    let bytes = image_generator
        .device
        .download_texture_data(&texture)
        .expect("readback");
    let pixel_count = (texture.width() * texture.height()) as usize;
    assert_eq!(bytes.len(), pixel_count * 8);

    let (mut opaque, mut partial) = (0, 0);
    for i in 0..pixel_count {
        let (r, g, b, a) = (
            f16_at(&bytes, i * 4),
            f16_at(&bytes, i * 4 + 1),
            f16_at(&bytes, i * 4 + 2),
            f16_at(&bytes, i * 4 + 3),
        );
        if a > 0.01 {
            // ストレートアルファなので、描かれたピクセルのrgbは常にテキストカラーそのもの。
            assert!(
                (r - 1.0).abs() < 0.01 && g.abs() < 0.01 && b.abs() < 0.01,
                "pixel {i}: rgb must not be premultiplied by coverage, got ({r}, {g}, {b}, {a})"
            );
            if a > 0.99 {
                opaque += 1;
            } else {
                partial += 1;
            }
        }
    }
    assert!(opaque > 0, "expected fully covered pixels");
    assert!(partial > 0, "expected anti-aliased (partially covered) pixels");
}

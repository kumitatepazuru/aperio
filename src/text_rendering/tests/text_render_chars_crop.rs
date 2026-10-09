// `run_render_chars`が文字ごとに切り抜いたテクスチャが、全体描画の該当領域と一致することの検証。

use gpu_util::image_generator::ImageGenerator;
use gpu_util::ImagePixelFormat;
use text_rendering::text_renderer::TextRenderer;
use text_rendering::TextSpec;

const BYTES_PER_PIXEL: usize = 8; // Rgba16Float

#[test]
fn per_char_crops_match_full_render() {
    let image_generator = match ImageGenerator::new(ImagePixelFormat::Rgba16Float) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("Skipping per_char_crops_match_full_render: no GPU ({e:#})");
            return;
        }
    };
    let mut renderer = TextRenderer::new(&image_generator).expect("renderer creation");

    let spec = TextSpec {
        text: "AB".to_string(),
        font_size: 48.0,
        ..Default::default()
    };
    let prepared = renderer
        .prepare_render_text(&spec)
        .expect("prepare should succeed")
        .expect("glyphs expected (is a system font available?)");

    let full = renderer.run_render_text(&prepared).expect("full render");
    let full_bytes = image_generator.device.download_texture_data(&full).unwrap();
    let full_width = full.width() as usize;

    let chars = renderer.run_render_chars(&prepared).expect("char render");
    assert_eq!(chars.len(), 2, "one crop per character");
    assert_eq!(chars[0].0.ch, 'A');
    assert_eq!(chars[1].0.ch, 'B');

    for (glyph, texture) in &chars {
        assert_eq!((texture.width(), texture.height()), (glyph.w, glyph.h));
        let crop = image_generator.device.download_texture_data(texture).unwrap();
        for row in 0..glyph.h as usize {
            let src_start = ((glyph.y as usize + row) * full_width + glyph.x as usize) * BYTES_PER_PIXEL;
            let dst_start = row * glyph.w as usize * BYTES_PER_PIXEL;
            let len = glyph.w as usize * BYTES_PER_PIXEL;
            assert_eq!(
                &crop[dst_start..dst_start + len],
                &full_bytes[src_start..src_start + len],
                "row {row} of '{}' should match the full render",
                glyph.ch
            );
        }
    }
}

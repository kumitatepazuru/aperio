// ラスターシェーダー(`ShaderKind::Raster`)のステップを、プラグインマネージャーのブレンドモードと
// 同じ組み立て方(`add_parallel([フレーム, レイヤー]).add_shader(ブレンド, params)`)で実行し、
// - 1枚目の入力(フレーム)が下地として残ること
// - aperio_blend.slang の頂点シェーダーがレイヤー矩形を正しい位置・大きさに配置すること
// - 不透明度が合成に掛かること
// - 2レイヤーを順に重ねられること
// を確認する。

use aperio_gpu::compiled_func::{CompiledFunc, CpuOutput};
use aperio_gpu::compiled_shader::CompiledShader;
use aperio_gpu::image_generate_builder::ImageGenerateBuilder;
use aperio_gpu::image_generator::layout::InputArity;
use aperio_gpu::image_generator::ImageGenerator;
use aperio_gpu::rhi::{AddressMode, FilterMode, SamplerOptions};
use aperio_gpu::ImagePixelFormat;
use std::sync::Arc;

const SHADERS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../python/aperio_plugin/shaders");
const BLEND_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../python/aperio_plugin/shaders/blend");

const CANVAS: u32 = 8;

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"))
}

/// 2x2 の単色レイヤーを出力するCPUステップ
fn solid_layer(rgba: [f32; 4]) -> ImageGenerateBuilder {
    let func = CompiledFunc::new(Box::new(move |_, _| {
        Ok(CpuOutput {
            data: rgba.repeat(4),
        })
    }));
    ImageGenerateBuilder::new().add_func(func, None, 2, 2)
}

/// frame_builder.blend_layer_params と同じ並び(forwardの各行 + center + alpha)
fn params(forward: [[f32; 3]; 3], alpha: f32) -> Vec<u8> {
    let values: [f32; 16] = [
        forward[0][0], forward[0][1], forward[0][2], 0.0, //
        forward[1][0], forward[1][1], forward[1][2], 0.0, //
        forward[2][0], forward[2][1], forward[2][2], 0.0, //
        0.0, 0.0, alpha, 0.0,
    ];
    bytemuck::cast_slice(&values).to_vec()
}

fn translate(tx: f32, ty: f32) -> [[f32; 3]; 3] {
    [[1.0, 0.0, tx], [0.0, 1.0, ty], [0.0, 0.0, 1.0]]
}

struct Fixture {
    generator: ImageGenerator,
    fill_black: Arc<CompiledShader>,
    normal: Arc<CompiledShader>,
}

impl Fixture {
    fn frame(&self) -> ImageGenerateBuilder {
        ImageGenerateBuilder::new().add_shader(self.fill_black.clone(), None, CANVAS, CANVAS)
    }

    fn blend(
        &self,
        frame: ImageGenerateBuilder,
        layer: ImageGenerateBuilder,
        params: Vec<u8>,
    ) -> ImageGenerateBuilder {
        ImageGenerateBuilder::new()
            .add_parallel(vec![frame, layer])
            .add_shader(self.normal.clone(), Some(params), CANVAS, CANVAS)
    }

    async fn render(&self, builder: ImageGenerateBuilder) -> Vec<[u8; 4]> {
        self.generator
            .generate_buf(builder)
            .await
            .expect("pipeline execution should succeed")
            .chunks_exact(4)
            .map(|p| [p[0], p[1], p[2], p[3]])
            .collect()
    }
}

fn assert_region(pixels: &[[u8; 4]], inside: impl Fn(u32, u32) -> bool, expected_inside: [u8; 4], label: &str) {
    for y in 0..CANVAS {
        for x in 0..CANVAS {
            let actual = pixels[(y * CANVAS + x) as usize];
            let expected = if inside(x, y) { expected_inside } else { [0, 0, 0, 255] };
            let close = actual
                .iter()
                .zip(expected.iter())
                .all(|(a, e)| (*a as i32 - *e as i32).abs() <= 1);
            assert!(close, "{label}: pixel ({x}, {y}) = {actual:?}, expected {expected:?}");
        }
    }
}

#[test]
fn blends_layers_with_a_raster_shader() {
    let rt = tokio::runtime::Runtime::new().expect("failed to build a tokio runtime");
    rt.block_on(run());
}

async fn run() {
    let generator = match ImageGenerator::new(ImagePixelFormat::Rgba16Float) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("Skipping blends_layers_with_a_raster_shader: no Vulkan device ({e:#})");
            return;
        }
    };

    let fill_black = CompiledShader::from_slang(
        "raster_test_fill_black",
        &read(&format!("{SHADERS_DIR}/fill_black.slang")),
        "main",
        &generator,
        generator.image_format(),
        InputArity::Fixed,
        &[SHADERS_DIR],
        &[],
        None,
    )
    .expect("fill_black should compile");
    let normal = CompiledShader::from_slang_raster(
        "raster_test_fallback_normal",
        &read(&format!("{BLEND_DIR}/fallback_normal.slang")),
        "vs_main",
        "fs_main",
        6,
        &generator,
        generator.image_format(),
        &[BLEND_DIR],
        &[],
        Some(&SamplerOptions {
            address_mode: AddressMode::ClampToEdge,
            filter: FilterMode::Linear,
        }),
    )
    .expect("fallback_normal should compile as a raster shader");
    assert!(normal.is_raster());

    let fx = Fixture {
        generator,
        fill_black: Arc::new(fill_black),
        normal: Arc::new(normal),
    };
    let red = [1.0, 0.0, 0.0, 1.0];

    // 移動なし: 2x2 のレイヤーはキャンバス中央 (3..=4, 3..=4) に置かれ、それ以外は黒い下地のまま
    let pixels = fx
        .render(fx.blend(fx.frame(), solid_layer(red), params(translate(0.0, 0.0), 1.0)))
        .await;
    assert_region(&pixels, |x, y| (3..=4).contains(&x) && (3..=4).contains(&y), [255, 0, 0, 255], "centered");

    // 移動 (+2, -1) かつ不透明度 0.5: 黒の上に半分だけ赤が乗る
    let pixels = fx
        .render(fx.blend(fx.frame(), solid_layer(red), params(translate(2.0, -1.0), 0.5)))
        .await;
    assert_region(&pixels, |x, y| (5..=6).contains(&x) && (2..=3).contains(&y), [128, 0, 0, 255], "translated half alpha");

    // 拡大率 2 倍: 4x4 の範囲 (2..=5) を覆う(頂点は四隅だけなので、内側は補間で埋まる)
    let scale2 = [[2.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 0.0, 1.0]];
    let pixels = fx
        .render(fx.blend(fx.frame(), solid_layer(red), params(scale2, 1.0)))
        .await;
    assert_region(&pixels, |x, y| (2..=5).contains(&x) && (2..=5).contains(&y), [255, 0, 0, 255], "scaled");

    // 2レイヤーを順に重ねる: 赤(中央) の上に、右へ1ずらした緑を重ねる
    let green = [0.0, 1.0, 0.0, 1.0];
    let frame = fx.blend(fx.frame(), solid_layer(red), params(translate(0.0, 0.0), 1.0));
    let frame = fx.blend(frame, solid_layer(green), params(translate(1.0, 0.0), 1.0));
    let pixels = fx.render(frame).await;
    for y in 0..CANVAS {
        for x in 0..CANVAS {
            let actual = pixels[(y * CANVAS + x) as usize];
            let expected = match (x, y) {
                (4..=5, 3..=4) => [0, 255, 0, 255],
                (3, 3..=4) => [255, 0, 0, 255],
                _ => [0, 0, 0, 255],
            };
            assert_eq!(actual, expected, "stacked: pixel ({x}, {y})");
        }
    }
}

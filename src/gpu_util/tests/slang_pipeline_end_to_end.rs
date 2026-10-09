// CompiledShader + ImageGenerateBuilder::add_shader + ImageGenerator::generate_buf
// という、プラグインが実際に使う経路をエンドツーエンドで検証する。
// aperio_bindings.slangのEffectResources<Params>パターン(ParameterBlock
// 経由の出力ストレージテクスチャ)を模したシェーダーを実際にコンパイル・実行し、
// 期待通りの色がCPUへ読み戻せることを確認する。

use gpu_util::compiled_shader::CompiledShader;
use gpu_util::image_generate_builder::ImageGenerateBuilder;
use gpu_util::image_generator::layout::InputArity;
use gpu_util::image_generator::ImageGenerator;
use gpu_util::ImagePixelFormat;

#[test]
fn runs_a_single_slang_step_end_to_end() {
    let rt = tokio::runtime::Runtime::new().expect("failed to build a tokio runtime");
    rt.block_on(run());
}

async fn run() {
    let generator = match ImageGenerator::new(ImagePixelFormat::Rgba8Unorm) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("Skipping runs_a_single_slang_step_end_to_end: no Vulkan device ({e:#})");
            return;
        }
    };

    let source = r#"
        struct EffectResources {
            [vk::image_format(APERIO_IMAGE_FORMAT)]
            RWTexture2D<float4> outputTex;
        };
        [vk::binding(0, 1)]
        ParameterBlock<EffectResources> res;

        [shader("compute")]
        [numthreads(16, 16, 1)]
        void main(uint3 gid : SV_DispatchThreadID) {
            uint w, h;
            res.outputTex.GetDimensions(w, h);
            if (gid.x >= w || gid.y >= h) {
                return;
            }
            res.outputTex[gid.xy] = float4(1.0, 0.5, 0.25, 1.0);
        }
    "#;

    let compiled = CompiledShader::from_slang(
        "slang_pipeline_e2e_solid_color",
        source,
        "main",
        &generator,
        generator.image_format(),
        InputArity::Fixed,
        &[],
        &[],
        None,
    )
    .expect("solid-color shader should compile");

    let width = 4u32;
    let height = 4u32;
    let builder = ImageGenerateBuilder::new().add_shader(compiled, None, width, height);

    let bytes = generator
        .generate_buf(builder)
        .await
        .expect("pipeline execution should succeed");

    assert_eq!(bytes.len(), (width * height * 4) as usize);

    for pixel in bytes.chunks_exact(4) {
        let [r, g, b, a] = [pixel[0], pixel[1], pixel[2], pixel[3]];
        assert_eq!(r, 255, "red channel mismatch");
        assert!((125..=128).contains(&g), "green channel out of range: {g}");
        assert!((61..=64).contains(&b), "blue channel out of range: {b}");
        assert_eq!(a, 255, "alpha channel mismatch");
    }
}

// compile_hlsl_compute + ImageGenerateBuilder::add_shader + ImageGenerator::generate_buf。
// HLSLで書いたコンピュートシェーダーが、Slangシェーダーと同じ経路(同じレイアウト規約)で
// 実行できることを確認する。

use gpu_util::compiled_hlsl::compile_hlsl_compute;
use gpu_util::image_generate_builder::ImageGenerateBuilder;
use gpu_util::image_generator::layout::InputArity;
use gpu_util::image_generator::ImageGenerator;
use gpu_util::ImagePixelFormat;

#[test]
fn runs_a_single_hlsl_step_end_to_end() {
    let rt = tokio::runtime::Runtime::new().expect("failed to build a tokio runtime");
    rt.block_on(run());
}

async fn run() {
    let generator = match ImageGenerator::new(ImagePixelFormat::Rgba8Unorm) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("Skipping runs_a_single_hlsl_step_end_to_end: no Vulkan device ({e:#})");
            return;
        }
    };

    // 出力は set 1 binding 0 のストレージテクスチャ(Slangのres.outputTexと同じ位置)。
    let source = r#"
        [[vk::binding(0, 1)]]
        [[vk::image_format(APERIO_IMAGE_FORMAT)]]
        RWTexture2D<float4> outputTex;

        [numthreads(16, 16, 1)]
        void main(uint3 gid : SV_DispatchThreadID) {
            uint w, h;
            outputTex.GetDimensions(w, h);
            if (gid.x >= w || gid.y >= h) {
                return;
            }
            outputTex[gid.xy] = float4(1.0, 0.5, 0.25, 1.0);
        }
    "#;

    let compiled = compile_hlsl_compute(
        "hlsl_pipeline_e2e_solid_color",
        source,
        "main",
        &generator,
        generator.image_format(),
        InputArity::Fixed,
        &[],
        &[],
        None,
    )
    .expect("solid-color HLSL shader should compile");

    let (width, height) = (4u32, 4u32);
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

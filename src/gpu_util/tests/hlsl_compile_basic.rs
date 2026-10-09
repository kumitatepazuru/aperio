// HLSL → SPIR-V(DXC)の基本的なコンパイルの検証。

use gpu_util::compiled_hlsl::{compile_hlsl_to_spirv, HlslStage};

const SPIRV_MAGIC: u32 = 0x0723_0203;

#[test]
fn compiles_trivial_compute_shader() {
    let source = r#"
        [[vk::binding(0, 0)]] RWStructuredBuffer<float4> output;

        [numthreads(4, 1, 1)]
        void main(uint3 tid : SV_DispatchThreadID) {
            output[tid.x] = float4(tid.x, 1.0, 2.0, 3.0);
        }
    "#;
    let spirv = compile_hlsl_to_spirv("trivial.hlsl", source, "main", HlslStage::Compute, &[], &[])
        .expect("trivial compute shader should compile");
    assert_eq!(spirv[0], SPIRV_MAGIC);
}

#[test]
fn applies_defines() {
    let source = r#"
        [[vk::binding(0, 0)]] RWStructuredBuffer<float> output;

        [numthreads(1, 1, 1)]
        void main() {
        #if SCALE == 3
            output[0] = 3.0;
        #else
        #error SCALE must be 3
        #endif
        }
    "#;
    compile_hlsl_to_spirv("defines.hlsl", source, "main", HlslStage::Compute, &[], &[("SCALE", "3")])
        .expect("the define should satisfy the #if");
    assert!(
        compile_hlsl_to_spirv("defines.hlsl", source, "main", HlslStage::Compute, &[], &[]).is_err(),
        "without the define the #error branch must fail"
    );
}

#[test]
fn keeps_the_entry_point_name() {
    // DXCはSlangと違い、エントリポイント名をSPIR-V内にそのまま残す。
    let source = r#"
        [[vk::binding(0, 0)]] RWStructuredBuffer<float> output;

        [numthreads(1, 1, 1)]
        void my_entry() { output[0] = 1.0; }
    "#;
    let spirv =
        compile_hlsl_to_spirv("entry.hlsl", source, "my_entry", HlslStage::Compute, &[], &[])
            .expect("shader should compile");
    let bytes: Vec<u8> = spirv.iter().flat_map(|w| w.to_le_bytes()).collect();
    assert!(
        bytes.windows(8).any(|w| w == b"my_entry"),
        "the SPIR-V should still contain the original entry point name"
    );
}

#[test]
fn reports_diagnostics_on_invalid_source() {
    let err = compile_hlsl_to_spirv(
        "broken.hlsl",
        "this is not HLSL",
        "main",
        HlslStage::Compute,
        &[],
        &[],
    )
    .expect_err("invalid HLSL must not compile");
    let message = format!("{err:#}");
    assert!(
        message.contains("broken.hlsl"),
        "diagnostics should mention the source name, got: {message}"
    );
}

// `compiled_shader::compile_slang_to_spirv`が実際にSlangソースをSPIR-Vへコンパイルできることを確認する。

use gpu_util::compiled_shader::compile_slang_to_spirv;

#[test]
fn compiles_trivial_compute_shader() {
    let source = r#"
        [shader("compute")]
        [numthreads(1, 1, 1)]
        void main(uint3 tid : SV_DispatchThreadID) {}
    "#;

    let spirv = compile_slang_to_spirv(
        "slang_compile_basic_test",
        "slang_compile_basic_test.slang",
        source,
        "main",
        &[],
        &[],
    )
    .expect("trivial compute shader should compile to SPIR-V");

    assert!(!spirv.is_empty(), "expected non-empty SPIR-V word stream");
    // SPIR-V モジュールは常にマジックナンバー 0x07230203 で始まる。
    assert_eq!(spirv[0], 0x0723_0203, "missing SPIR-V magic number");
}

#[test]
fn reports_diagnostics_on_invalid_source() {
    let source = "this is not valid slang source {{{";

    let result = compile_slang_to_spirv(
        "slang_compile_basic_invalid",
        "slang_compile_basic_invalid.slang",
        source,
        "main",
        &[],
        &[],
    );

    assert!(result.is_err(), "invalid source must fail to compile");
    let message = result.unwrap_err().to_string();
    assert!(
        !message.is_empty(),
        "error message should include Slang diagnostics"
    );
}

// HLSLをDXC(`-spirv`)でVulkan用SPIR-Vへコンパイルする。得られるSPIR-Vは、Slangの
// 出力と同じ`rhi::Device::create_*_pipeline`にそのまま渡せる。
//
// Slangとの違い:
// - エントリポイント名はSPIR-Vにもそのまま残る(Slangは常に"main")。
//   パイプライン生成時には`compile_hlsl_to_spirv`に渡したのと同じ名前を指定すること。
// - リソースの束縛位置は`[[vk::binding(binding, set)]]`で明示する(HLSLの`register`は無視される)。
// - クリップ空間はD3D準拠(NDCのy+が上)。RHIはビューポートを反転して描画するので、
//   D3D向けに書かれたシェーダーがそのまま同じ向きで描ける。

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use anyhow::{bail, Result};

use crate::compiled_shader::CompiledShader;
use crate::image_generator::layout::InputArity;
use crate::image_generator::ImageGenerator;
use crate::image_pixel_format::ImagePixelFormat;
use crate::rhi::SamplerOptions;

#[allow(
    non_upper_case_globals,
    non_camel_case_types,
    non_snake_case,
    dead_code
)]
mod ffi {
    include!(concat!(env!("OUT_DIR"), "/hlsl_bindings.rs"));
}

/// シェーダーステージ(DXCのターゲットプロファイルの接頭辞に対応する)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HlslStage {
    Vertex,
    Fragment,
    Compute,
}

impl HlslStage {
    /// Shader Model 6.2 のターゲットプロファイル。
    fn profile(self) -> &'static str {
        match self {
            HlslStage::Vertex => "vs_6_2",
            HlslStage::Fragment => "ps_6_2",
            HlslStage::Compute => "cs_6_2",
        }
    }
}

/// hlsl_shim_compile_to_spirvが返す結果へのRAIIガード。Dropで必ず
/// hlsl_shim_result_freeを呼ぶため、?による早期returnでもリークしない。
struct CompileResultGuard(*mut ffi::HlslShimCompileResult);

impl Drop for CompileResultGuard {
    fn drop(&mut self) {
        unsafe { ffi::hlsl_shim_result_free(self.0) };
    }
}

/// HLSLソースをSPIR-Vへコンパイルする。
///
/// - source_name: 診断メッセージ表示用の仮想パス。
/// - include_dirs: `#include`解決に使うディレクトリ一覧。
/// - defines: プリプロセッサマクロ(`-D name=value`相当)。
///
/// 戻り値はSPIR-Vのワード列(4バイト単位)。
pub fn compile_hlsl_to_spirv(
    source_name: &str,
    source: &str,
    entry_point: &str,
    stage: HlslStage,
    include_dirs: &[&str],
    defines: &[(&str, &str)],
) -> Result<Vec<u32>> {
    let source_name_c = CString::new(source_name)?;
    let source_c = CString::new(source)?;
    let entry_point_c = CString::new(entry_point)?;
    let profile_c = CString::new(stage.profile())?;

    let include_c: Vec<CString> = include_dirs
        .iter()
        .map(|p| CString::new(*p))
        .collect::<Result<_, _>>()?;
    let include_ptrs: Vec<*const c_char> = include_c.iter().map(|s| s.as_ptr()).collect();

    let define_name_c: Vec<CString> = defines
        .iter()
        .map(|(n, _)| CString::new(*n))
        .collect::<Result<_, _>>()?;
    let define_value_c: Vec<CString> = defines
        .iter()
        .map(|(_, v)| CString::new(*v))
        .collect::<Result<_, _>>()?;
    let define_name_ptrs: Vec<*const c_char> = define_name_c.iter().map(|s| s.as_ptr()).collect();
    let define_value_ptrs: Vec<*const c_char> = define_value_c.iter().map(|s| s.as_ptr()).collect();

    let result_ptr = unsafe {
        ffi::hlsl_shim_compile_to_spirv(
            source_name_c.as_ptr(),
            source_c.as_ptr(),
            entry_point_c.as_ptr(),
            profile_c.as_ptr(),
            include_ptrs.as_ptr(),
            include_ptrs.len(),
            define_name_ptrs.as_ptr(),
            define_value_ptrs.as_ptr(),
            define_name_ptrs.len(),
        )
    };
    if result_ptr.is_null() {
        bail!("hlsl_shim_compile_to_spirv returned a null result unexpectedly");
    }
    let guard = CompileResultGuard(result_ptr);

    let success = unsafe { ffi::hlsl_shim_result_success(guard.0) } != 0;

    // C側のポインタは`guard`がdropされるまでしか有効でないため、先にコピーしておく。
    let diagnostics = unsafe {
        let ptr = ffi::hlsl_shim_result_diagnostics(guard.0);
        if ptr.is_null() {
            String::new()
        } else {
            CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    };

    if !success {
        bail!("HLSL compilation of '{source_name}' failed:\n{diagnostics}");
    }

    let spirv = unsafe {
        let data = ffi::hlsl_shim_result_spirv_data(guard.0);
        let len = ffi::hlsl_shim_result_spirv_word_count(guard.0);
        if data.is_null() || len == 0 {
            bail!(
                "HLSL compilation of '{source_name}' reported success but returned no SPIR-V words"
            );
        }
        std::slice::from_raw_parts(data, len).to_vec()
    };

    if !diagnostics.is_empty() {
        log::warn!("HLSL compilation of '{source_name}' produced warnings:\n{diagnostics}");
    }

    Ok(spirv)
}

/// HLSLのコンピュートシェーダーをコンパイルし、Slangシェーダーと同じ
/// `ImageGenerateBuilder::add_shader`で実行できる形にする。
///
/// リソースはSlangと同じ規約で`[[vk::binding(binding, set)]]`により
/// 入力=set 0、出力・サンプラー・params=set 1に束縛しておくこと。
/// nameはパイプラインキャッシュのキーとしても使われるため、同じソースを異なるdefinesで
/// コンパイルする場合は必ず別のnameを渡すこと。
pub fn compile_hlsl_compute(
    name: &str,
    source: &str,
    entry_point: &str,
    generator: &ImageGenerator,
    output_format: ImagePixelFormat,
    input_arity: InputArity,
    include_dirs: &[&str],
    defines: &[(&str, &str)],
    sampler_options: Option<&SamplerOptions>,
) -> Result<CompiledShader> {
    // [[vk::image_format(APERIO_IMAGE_FORMAT)]]が参照するマクロを自動的に注入する。
    let mut all_defines: Vec<(&str, &str)> = defines.to_vec();
    all_defines.push((
        "APERIO_IMAGE_FORMAT",
        output_format.to_slang_image_format_literal(),
    ));

    let spirv = compile_hlsl_to_spirv(
        &format!("{name}.hlsl"),
        source,
        entry_point,
        HlslStage::Compute,
        include_dirs,
        &all_defines,
    )?;
    CompiledShader::from_spirv(
        name,
        spirv,
        entry_point,
        generator,
        output_format,
        input_arity,
        sampler_options,
    )
}

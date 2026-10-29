// cpp/src/slang_shim.cpp (Slang C API) のrust wrapper
//
// [vk::binding(0, 0)] ParameterBlock<...> inputs;
// [vk::binding(0, 1)] ParameterBlock<...> res;
// で常にinputs=set0・res=set1に固定されており、setの自動割り当てに依存しない。

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::Mutex;

use anyhow::{bail, Result};

use crate::image_generator::layout::InputArity;
use crate::image_generator::ImageGenerator;
use crate::image_pixel_format::ImagePixelFormat;
use crate::rhi::{Sampler, SamplerOptions};

/// slang_shim.cppは実行体内で共有される単一のslang::IGlobalSessionを持つ
/// (C++11のmagic staticsで初期化は保護されているが、その後のcreateSession
/// 等の並行呼び出しがスレッドセーフである保証はない)。
static COMPILE_LOCK: Mutex<()> = Mutex::new(());

#[allow(
    non_upper_case_globals,
    non_camel_case_types,
    non_snake_case,
    dead_code
)]
mod ffi {
    include!(concat!(env!("OUT_DIR"), "/slang_bindings.rs"));
}

/// slang_shim_compile_to_spirvが返す結果へのRAIIガード。Dropで必ず
/// slang_shim_result_freeを呼ぶため、?による早期returnでもリークしない。
struct CompileResultGuard(*mut ffi::SlangShimCompileResult);

impl Drop for CompileResultGuard {
    fn drop(&mut self) {
        unsafe { ffi::slang_shim_result_free(self.0) };
    }
}

/// SlangソースをSPIR-Vへコンパイルする。
///
/// - module_name: モジュール識別子(パイプラインキャッシュのキー等に使う)。
/// - module_path: 診断メッセージ表示用の仮想パス。
/// - search_paths: `import`解決に使うディレクトリ一覧。
/// - defines: プリプロセッサマクロ(`#define name value`相当)。
///
/// 戻り値はSPIR-Vのワード列(4バイト単位)。C側のバッファはこの関数の
/// 終了時に解放されるため、呼び出し後は所有権を持つ`Vec<u32>`のみが残る。
pub fn compile_slang_to_spirv(
    module_name: &str,
    module_path: &str,
    source: &str,
    entry_point: &str,
    search_paths: &[&str],
    defines: &[(&str, &str)],
) -> Result<Vec<u32>> {
    let module_name_c = CString::new(module_name)?;
    let module_path_c = CString::new(module_path)?;
    let source_c = CString::new(source)?;
    let entry_point_c = CString::new(entry_point)?;

    let search_path_cs: Vec<CString> = search_paths
        .iter()
        .map(|s| CString::new(*s))
        .collect::<std::result::Result<_, _>>()?;
    let search_path_ptrs: Vec<*const c_char> = search_path_cs.iter().map(|s| s.as_ptr()).collect();

    let define_name_cs: Vec<CString> = defines
        .iter()
        .map(|(k, _)| CString::new(*k))
        .collect::<std::result::Result<_, _>>()?;
    let define_value_cs: Vec<CString> = defines
        .iter()
        .map(|(_, v)| CString::new(*v))
        .collect::<std::result::Result<_, _>>()?;
    let define_name_ptrs: Vec<*const c_char> = define_name_cs.iter().map(|s| s.as_ptr()).collect();
    let define_value_ptrs: Vec<*const c_char> =
        define_value_cs.iter().map(|s| s.as_ptr()).collect();

    let _guard = COMPILE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let result_ptr = unsafe {
        ffi::slang_shim_compile_to_spirv(
            module_name_c.as_ptr(),
            module_path_c.as_ptr(),
            source_c.as_ptr(),
            entry_point_c.as_ptr(),
            search_path_ptrs.as_ptr(),
            search_path_ptrs.len(),
            define_name_ptrs.as_ptr(),
            define_value_ptrs.as_ptr(),
            define_value_ptrs.len(),
        )
    };

    // ヘッダの仕様上、コンパイル自体が失敗した場合でも常に非NULLが返るが、
    // FFI境界を越える値として念のため確認する。
    if result_ptr.is_null() {
        bail!("slang_shim_compile_to_spirv returned a null result unexpectedly");
    }
    let guard = CompileResultGuard(result_ptr);

    let success = unsafe { ffi::slang_shim_result_success(guard.0) } != 0;

    // C側のポインタは`guard`がdropされる(=slang_shim_result_freeが呼ばれる)まで
    // しか有効でないため、所有権のある`String`/`Vec`へ先にコピーしておく。
    let diagnostics = unsafe {
        let ptr = ffi::slang_shim_result_diagnostics(guard.0);
        if ptr.is_null() {
            String::new()
        } else {
            CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    };

    if !success {
        bail!("Slang compilation of '{module_name}' failed:\n{diagnostics}");
    }

    let spirv = unsafe {
        let data = ffi::slang_shim_result_spirv_data(guard.0);
        let len = ffi::slang_shim_result_spirv_word_count(guard.0);
        if data.is_null() || len == 0 {
            bail!(
                "Slang compilation of '{module_name}' reported success but returned no SPIR-V words"
            );
        }
        std::slice::from_raw_parts(data, len).to_vec()
    };

    if !diagnostics.is_empty() {
        log::warn!("Slang compilation of '{module_name}' produced warnings:\n{diagnostics}");
    }

    Ok(spirv)
}

/// コンパイル済みのSPIR-Vシェーダー1つ分。
pub struct CompiledShader {
    /// パイプラインキャッシュのキーにもなる名前。
    pub name: String,
    pub spirv: Vec<u32>,
    pub entry_point: String,
    /// このシェーダーの出力ストレージテクスチャが実際に使うフォーマット。
    pub output_format: ImagePixelFormat,
    pub input_arity: InputArity,
    /// Someならresが`EffectResourcesSampler<Params>`(サンプラー付き)、
    /// NoneならEffectResources<Params>/EffectResourcesNoParams。
    pub sampler: Option<Sampler>,
}

impl CompiledShader {
    /// slang sourceをコンパイルし、(指定されていれば)サンプラーを生成する。
    ///
    /// nameはパイプラインキャッシュのキーとしても使われるため、
    /// 同じソースを異なるdefinesでコンパイルする場合は必ず別のnameを渡すこと。
    pub fn from_slang(
        name: &str,
        source: &str,
        entry_point: &str,
        generator: &ImageGenerator,
        output_format: ImagePixelFormat,
        input_arity: InputArity,
        search_paths: &[&str],
        defines: &[(&str, &str)],
        sampler_options: Option<&SamplerOptions>,
    ) -> Result<Self> {
        // [vk::image_format(APERIO_IMAGE_FORMAT)]が
        // 参照するマクロを自動的に注入する(呼び出し側が毎回指定しなくてよいように)。
        let mut all_defines: Vec<(&str, &str)> = defines.to_vec();
        all_defines.push((
            "APERIO_IMAGE_FORMAT",
            output_format.to_slang_image_format_literal(),
        ));

        let spirv = compile_slang_to_spirv(
            name,
            &format!("{name}.slang"),
            source,
            entry_point,
            search_paths,
            &all_defines,
        )?;

        Self::from_spirv(
            name,
            spirv,
            entry_point,
            generator,
            output_format,
            input_arity,
            sampler_options,
        )
    }

    /// コンパイル済みのSPIR-V(Slang / HLSLのどちらから作ったものでもよい)から作り、
    /// (指定されていれば)サンプラーを生成する。
    ///
    /// `entry_point`はSPIR-V内のエントリポイント名(Slangは常に"main"、HLSLはソースの名前のまま)。
    pub fn from_spirv(
        name: &str,
        spirv: Vec<u32>,
        entry_point: &str,
        generator: &ImageGenerator,
        output_format: ImagePixelFormat,
        input_arity: InputArity,
        sampler_options: Option<&SamplerOptions>,
    ) -> Result<Self> {
        let sampler = sampler_options
            .map(|options| generator.device.create_sampler(options))
            .transpose()?;

        Ok(Self {
            name: name.to_string(),
            spirv,
            entry_point: entry_point.to_string(),
            output_format,
            input_arity,
            sampler,
        })
    }
}

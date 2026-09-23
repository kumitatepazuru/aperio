// cpp/src/slang_shim.cpp (Slang C API) の安全なRustラッパー。SPIR-Vへの
// コンパイルのみを扱う。

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::Mutex;

use anyhow::{bail, Result};

/// `slang_shim.cpp`は実行体内で共有される単一の`slang::IGlobalSession`を持つ
/// (C++11のmagic staticsで初期化は保護されているが、その後の`createSession`
/// 等の並行呼び出しがスレッドセーフである保証はない)。
static COMPILE_LOCK: Mutex<()> = Mutex::new(());

#[allow(non_upper_case_globals, non_camel_case_types, non_snake_case, dead_code)]
mod ffi {
    include!(concat!(env!("OUT_DIR"), "/slang_bindings.rs"));
}

/// `slang_shim_compile_to_spirv`が返す結果へのRAIIガード。Dropで必ず
/// `slang_shim_result_free`を呼ぶため、`?`による早期returnでもリークしない。
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

    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

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

#pragma once
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Slangコンパイル1回ぶんの結果(成功/失敗を問わず生成される)。
// 呼び出し側は必ず slang_shim_result_free で解放すること。
typedef struct SlangShimCompileResult SlangShimCompileResult;

// `source`(Slangソース文字列)を`entry_point_name`をエントリポイントとしてSPIR-Vへコンパイルする。
//
// - module_name: モジュール識別子(キャッシュ用)。
// - module_path: モジュールの仮想パス(診断メッセージ内のファイル名表示・キャッシュキーの補助に使われる)。
// - search_paths / search_path_count: `import`解決に使うディレクトリ検索パス一覧。
// - define_names / define_values / define_count: プリプロセッサマクロ(`#define name value`相当)。
//
// 戻り値は常に非NULL(コンパイル自体が失敗した場合も診断メッセージ付きの結果として返る)。
SlangShimCompileResult* slang_shim_compile_to_spirv(
    const char* module_name,
    const char* module_path,
    const char* source,
    const char* entry_point_name,
    const char* const* search_paths,
    size_t search_path_count,
    const char* const* define_names,
    const char* const* define_values,
    size_t define_count);

// コンパイルが成功したか(SPIR-Vが取得できたか)。0=失敗、非0=成功。
int slang_shim_result_success(const SlangShimCompileResult* result);

// 成功時のSPIR-Vワード列(4バイト単位)へのポインタ。失敗時はNULL。
const uint32_t* slang_shim_result_spirv_data(const SlangShimCompileResult* result);

// SPIR-Vのワード数(uint32_t単位)。失敗時は0。
size_t slang_shim_result_spirv_word_count(const SlangShimCompileResult* result);

// コンパイル中に蓄積された診断メッセージ(エラー・警告)。メッセージが無ければ空文字列(NULLにはならない)。
// NUL終端されたUTF-8文字列で、resultが解放されるまで有効。
const char* slang_shim_result_diagnostics(const SlangShimCompileResult* result);

// `slang_shim_compile_to_spirv`が返した結果を解放する。
void slang_shim_result_free(SlangShimCompileResult* result);

#ifdef __cplusplus
}
#endif

#pragma once
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// HLSLコンパイル1回ぶんの結果(成功/失敗を問わず生成される)。
// 呼び出し側は必ず hlsl_shim_result_free で解放すること。
typedef struct HlslShimCompileResult HlslShimCompileResult;

// `source`(HLSLソース文字列)をDXCでSPIR-V(Vulkan 1.2)へコンパイルする。
//
// - source_name: 診断メッセージ内のファイル名表示に使う仮想パス。
// - entry_point_name: エントリポイント名。生成されるSPIR-Vのエントリポイント名もこの名前のまま
//   (Slangと違い"main"に変更されない)。
// - target_profile: DXCのターゲットプロファイル("vs_6_2" / "ps_6_2" / "cs_6_2"など)。
// - include_dirs / include_dir_count: `#include`解決に使うディレクトリ検索パス一覧。
// - define_names / define_values / define_count: プリプロセッサマクロ(`-D name=value`相当)。
//
// 戻り値は常に非NULL(コンパイル自体が失敗した場合も診断メッセージ付きの結果として返る)。
HlslShimCompileResult* hlsl_shim_compile_to_spirv(
    const char* source_name,
    const char* source,
    const char* entry_point_name,
    const char* target_profile,
    const char* const* include_dirs,
    size_t include_dir_count,
    const char* const* define_names,
    const char* const* define_values,
    size_t define_count);

// コンパイルが成功したか(SPIR-Vが取得できたか)。0=失敗、非0=成功。
int hlsl_shim_result_success(const HlslShimCompileResult* result);

// 成功時のSPIR-Vワード列(4バイト単位)へのポインタ。失敗時はNULL。
const uint32_t* hlsl_shim_result_spirv_data(const HlslShimCompileResult* result);

// SPIR-Vのワード数(uint32_t単位)。失敗時は0。
size_t hlsl_shim_result_spirv_word_count(const HlslShimCompileResult* result);

// コンパイル中の診断メッセージ(エラー・警告)。メッセージが無ければ空文字列(NULLにはならない)。
// NUL終端されたUTF-8文字列で、resultが解放されるまで有効。
const char* hlsl_shim_result_diagnostics(const HlslShimCompileResult* result);

// `hlsl_shim_compile_to_spirv`が返した結果を解放する。
void hlsl_shim_result_free(HlslShimCompileResult* result);

#ifdef __cplusplus
}
#endif

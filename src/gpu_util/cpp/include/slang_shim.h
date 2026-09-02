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
// - search_paths / search_path_count: `import`解決に使うディレクトリの検索パス一覧。
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

// D3D12(DXIL)向けのコンパイル結果。SPIR-Vと違い、bgfxのD3D12レンダラーは各リソースの
// レジスタ番号(regIndex)をシェーダーバイナリのreflectionブロックにそのまま書けば済み、
// Vulkanのようなバインディング書き換え(post-process rewrite)が不要。そのかわり
// bgfxのD3D12ルートシグネチャは固定でregister space 0のみを対象にしているため、
// Slangが`ParameterBlock<T>`を(Vulkanのdescriptor set相当として)別のregister spaceへ
// 割り当てていないかを呼び出し側で確認する必要がある(`binding_space`アクセサ参照)。
typedef struct SlangShimDxilCompileResult SlangShimDxilCompileResult;

// `source`を`entry_point_name`をエントリポイントとしてDXILへコンパイルし、同時に
// トップレベルのシェーダーパラメータ(`ParameterBlock<T>`等)を再帰的に展開した
// 個々のリソース(Texture2D/RWTexture2D/SamplerState/StructuredBuffer等の葉ノード)の
// バインディング情報(名前・カテゴリ・space・register)を収集する。
// 戻り値は常に非NULL(コンパイル自体が失敗した場合も診断メッセージ付きの結果として返る)。
SlangShimDxilCompileResult* slang_shim_compile_to_dxil(
    const char* module_name,
    const char* module_path,
    const char* source,
    const char* entry_point_name,
    const char* const* search_paths,
    size_t search_path_count,
    const char* const* define_names,
    const char* const* define_values,
    size_t define_count);

int slang_shim_dxil_result_success(const SlangShimDxilCompileResult* result);
const uint8_t* slang_shim_dxil_result_data(const SlangShimDxilCompileResult* result);
size_t slang_shim_dxil_result_byte_count(const SlangShimDxilCompileResult* result);
const char* slang_shim_dxil_result_diagnostics(const SlangShimDxilCompileResult* result);

// 展開済みリソースバインディングの数。
size_t slang_shim_dxil_result_binding_count(const SlangShimDxilCompileResult* result);
// `res.samp`のようなドット区切りの完全修飾名(NUL終端、resultが生きている間だけ有効)。
const char* slang_shim_dxil_result_binding_name(const SlangShimDxilCompileResult* result, size_t index);
// gpu_util独自の簡略化した分類(Slangのバージョン依存の生の列挙値をそのまま渡さない):
// 0=unknown, 1=ShaderResource(HLSL `t`), 2=UnorderedAccess(HLSL `u`),
// 3=SamplerState(HLSL `s`), 4=ConstantBuffer(HLSL `b`)。
int slang_shim_dxil_result_binding_category(const SlangShimDxilCompileResult* result, size_t index);
// HLSLの`register(t0, spaceN)`の`space`部分。bgfxのD3D12ルートシグネチャはspace 0のみを
// 対象にしているため、呼び出し側はここが常に0であることを確認する必要がある。
uint32_t slang_shim_dxil_result_binding_space(const SlangShimDxilCompileResult* result, size_t index);
// `register(tN, ...)`の`N`部分(カテゴリ内でのレジスタ番号)。
uint32_t slang_shim_dxil_result_binding_register(const SlangShimDxilCompileResult* result, size_t index);
// 以下4つはフィールド宣言をソースレベルで再構築するための追加情報
// (category が ShaderResource/UnorderedAccess のリソースにのみ意味を持つ)。
// shape: 0=unknown, 1=Texture2D系, 2=StructuredBuffer系。
int slang_shim_dxil_result_binding_shape(const SlangShimDxilCompileResult* result, size_t index);
// access: 0=unknown, 1=読み取り専用(Texture2D/StructuredBuffer), 2=読み書き(RWTexture2D/RWStructuredBuffer)。
int slang_shim_dxil_result_binding_access(const SlangShimDxilCompileResult* result, size_t index);
// `StructuredBuffer<T>`の`T`部分の型名。shapeがStructuredBuffer系以外では空文字列。
const char* slang_shim_dxil_result_binding_element_type_name(const SlangShimDxilCompileResult* result, size_t index);
// `Texture2D<float4> tex[]`のような可変長配列宣言なら非0。
int slang_shim_dxil_result_binding_is_array(const SlangShimDxilCompileResult* result, size_t index);

// `slang_shim_compile_to_dxil`が返した結果を解放する。
void slang_shim_dxil_result_free(SlangShimDxilCompileResult* result);

#ifdef __cplusplus
}
#endif

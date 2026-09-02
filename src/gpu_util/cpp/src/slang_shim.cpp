#include "slang_shim.h"

#include <slang-com-helper.h>
#include <slang-com-ptr.h>
#include <slang.h>

#include <cstring>
#include <string>
#include <vector>

struct SlangShimCompileResult {
    bool success = false;
    std::vector<uint32_t> spirv;
    std::string diagnostics;
};

namespace {

// グローバルセッションは実行体内で使い回してよい(Slangのドキュメント通り、
// 最初の1回だけ確立すればよい接続)ため、static localで一度だけ作る。
// C++11のmagic staticsによりスレッドセーフに初期化される。
Slang::ComPtr<slang::IGlobalSession>& global_session() {
    static Slang::ComPtr<slang::IGlobalSession> session = [] {
        Slang::ComPtr<slang::IGlobalSession> s;
        slang::createGlobalSession(s.writeRef());
        return s;
    }();
    return session;
}

void append_diagnostics(std::string& out, slang::IBlob* blob) {
    if (blob == nullptr || blob->getBufferSize() == 0) {
        return;
    }
    if (!out.empty()) {
        out += '\n';
    }
    out.append(static_cast<const char*>(blob->getBufferPointer()), blob->getBufferSize());
}

}  // namespace

SlangShimCompileResult* slang_shim_compile_to_spirv(
    const char* module_name,
    const char* module_path,
    const char* source,
    const char* entry_point_name,
    const char* const* search_paths,
    size_t search_path_count,
    const char* const* define_names,
    const char* const* define_values,
    size_t define_count) {
    auto* result = new SlangShimCompileResult();

    slang::IGlobalSession* globalSession = global_session();
    if (globalSession == nullptr) {
        result->diagnostics = "Failed to create Slang global session";
        return result;
    }

    slang::TargetDesc targetDesc = {};
    targetDesc.format = SLANG_SPIRV;
    // nagaは1.4以降のprofileで生成されるOpCopyLogicalが対応していないため1.3に固定する。
    // 詳細：https://github.com/gfx-rs/wgpu/issues/8742
    targetDesc.profile = globalSession->findProfile("spirv_1_3");

    std::vector<slang::PreprocessorMacroDesc> macros;
    macros.reserve(define_count);
    for (size_t i = 0; i < define_count; ++i) {
        macros.push_back({define_names[i], define_values[i]});
    }

    slang::CompilerOptionEntry emitSpirvDirectly = {
        slang::CompilerOptionName::EmitSpirvMethod,
        {slang::CompilerOptionValueKind::Int, SLANG_EMIT_SPIRV_DIRECTLY, 0, nullptr, nullptr}};

    slang::SessionDesc sessionDesc = {};
    sessionDesc.targets = &targetDesc;
    sessionDesc.targetCount = 1;
    sessionDesc.searchPaths = search_paths;
    sessionDesc.searchPathCount = static_cast<SlangInt>(search_path_count);
    sessionDesc.preprocessorMacros = macros.empty() ? nullptr : macros.data();
    sessionDesc.preprocessorMacroCount = static_cast<SlangInt>(macros.size());
    sessionDesc.compilerOptionEntries = &emitSpirvDirectly;
    sessionDesc.compilerOptionEntryCount = 1;

    Slang::ComPtr<slang::ISession> session;
    if (SLANG_FAILED(globalSession->createSession(sessionDesc, session.writeRef()))) {
        result->diagnostics = "Failed to create Slang compile session";
        return result;
    }

    Slang::ComPtr<slang::IModule> module;
    {
        Slang::ComPtr<slang::IBlob> diagnosticsBlob;
        module = session->loadModuleFromSourceString(module_name, module_path, source,
                                                       diagnosticsBlob.writeRef());
        append_diagnostics(result->diagnostics, diagnosticsBlob);
    }
    if (module == nullptr) {
        return result;
    }

    Slang::ComPtr<slang::IEntryPoint> entryPoint;
    module->findEntryPointByName(entry_point_name, entryPoint.writeRef());
    if (entryPoint == nullptr) {
        result->diagnostics += "\nEntry point not found: ";
        result->diagnostics += entry_point_name;
        return result;
    }

    Slang::ComPtr<slang::IComponentType> composedProgram;
    {
        slang::IComponentType* components[] = {module, entryPoint};
        Slang::ComPtr<slang::IBlob> diagnosticsBlob;
        SlangResult compositeResult = session->createCompositeComponentType(
            components, 2, composedProgram.writeRef(), diagnosticsBlob.writeRef());
        append_diagnostics(result->diagnostics, diagnosticsBlob);
        if (SLANG_FAILED(compositeResult)) {
            return result;
        }
    }

    Slang::ComPtr<slang::IComponentType> linkedProgram;
    {
        Slang::ComPtr<slang::IBlob> diagnosticsBlob;
        SlangResult linkResult =
            composedProgram->link(linkedProgram.writeRef(), diagnosticsBlob.writeRef());
        append_diagnostics(result->diagnostics, diagnosticsBlob);
        if (SLANG_FAILED(linkResult)) {
            return result;
        }
    }

    Slang::ComPtr<slang::IBlob> spirvBlob;
    {
        Slang::ComPtr<slang::IBlob> diagnosticsBlob;
        SlangResult codeResult = linkedProgram->getEntryPointCode(
            0, 0, spirvBlob.writeRef(), diagnosticsBlob.writeRef());
        append_diagnostics(result->diagnostics, diagnosticsBlob);
        if (SLANG_FAILED(codeResult) || spirvBlob == nullptr) {
            return result;
        }
    }

    const size_t byteSize = spirvBlob->getBufferSize();
    const size_t wordCount = byteSize / sizeof(uint32_t);
    result->spirv.resize(wordCount);
    std::memcpy(result->spirv.data(), spirvBlob->getBufferPointer(), wordCount * sizeof(uint32_t));
    result->success = true;

    return result;
}

// SlangのSlangParameterCategoryをそのままFFI越しに渡すとRust側がSlangのバージョン依存の
// 生の列挙値を知る必要が出るため、gpu_utilが実際に扱う4種類だけに絞った独自分類へ変換する。
// 値は`slang_shim.h`のコメントと一致させること。
enum SlangShimResourceKind {
    kSlangShimResourceUnknown = 0,
    kSlangShimResourceShaderResource = 1,   // HLSL `t`レジスタ (Texture2D, 読み取り専用StructuredBuffer)
    kSlangShimResourceUnorderedAccess = 2,  // HLSL `u`レジスタ (RWTexture2D, RWStructuredBuffer)
    kSlangShimResourceSampler = 3,          // HLSL `s`レジスタ (SamplerState)
    kSlangShimResourceConstantBuffer = 4,   // HLSL `b`レジスタ (cbuffer/ConstantBuffer<T>)
};

// `SlangResourceShape`もSlangのバージョン依存の生の値をそのまま渡さず、gpu_utilが
// フィールド宣言を再構築する際に区別が必要な2種類だけに絞って独自分類する
// (呼び出し側が`ParameterBlock`の実際の型構造を知らなくても、`Texture2D<float4>`/
// `RWTexture2D<float4>`/`StructuredBuffer<T>`のようなフィールド宣言を機械的に
// 再構築できるようにするため。詳細は`slang_compiler::merge_input_and_res_parameter_blocks_for_d3d12`
// のコメント参照)。
enum SlangShimResourceShape {
    kSlangShimShapeUnknown = 0,
    kSlangShimShapeTexture2D = 1,
    kSlangShimShapeStructuredBuffer = 2,
};

enum SlangShimResourceAccess {
    kSlangShimAccessUnknown = 0,
    kSlangShimAccessRead = 1,
    kSlangShimAccessReadWrite = 2,
};

struct SlangShimResourceBinding {
    std::string name;
    int category = kSlangShimResourceUnknown;
    uint32_t space = 0;
    uint32_t index = 0;
    // 以下はフィールド宣言の再構築に使う追加情報(Sampler/ConstantBufferでは意味を
    // 持たない): `shape`はTexture2D/RWTexture2D vs StructuredBuffer/RWStructuredBufferの
    // 区別、`access`はRead vs ReadWrite(RWTexture2D等)の区別、`elementTypeName`は
    // `StructuredBuffer<T>`の`T`部分の型名(Texture2D側は常に`float4`というgpu_util自身の
    // 既定を使うため取得不要)、`isArray`は`Texture2D<float4> tex[]`のような可変長配列
    // 宣言かどうか。
    int shape = kSlangShimShapeUnknown;
    int access = kSlangShimAccessUnknown;
    std::string elementTypeName;
    bool isArray = false;
};

struct SlangShimDxilCompileResult {
    bool success = false;
    std::vector<uint8_t> dxil;
    std::string diagnostics;
    std::vector<SlangShimResourceBinding> bindings;
};

namespace {

// `varLayout`が指すシェーダーパラメータを再帰的に展開し、Texture/RWTexture/SamplerState/
// StructuredBuffer等の「葉」に到達するたびに1エントリを`out`へ積む。
// `ParameterBlock<T>`/`ConstantBuffer<T>`は中身の構造体を透過的に展開する
// (Vulkan版でSPIR-Vの`OpName`から`res.samp`のようなドット区切り名を組み立てていたのと
// 同じ考え方を、Slangのreflection APIを直接歩くことで実現する。DXILバイトコード自体を
// 自前でパースする必要が無い分、SPIR-V側より単純)。
void collectBindings(slang::VariableLayoutReflection* varLayout, const std::string& namePrefix,
                      std::vector<SlangShimResourceBinding>& out) {
    if (varLayout == nullptr) {
        return;
    }

    const char* fieldName = varLayout->getName();
    std::string name = namePrefix.empty()
                            ? (fieldName != nullptr ? fieldName : "")
                            : (fieldName != nullptr ? namePrefix + "." + fieldName : namePrefix);

    slang::TypeLayoutReflection* typeLayout = varLayout->getTypeLayout();
    if (typeLayout == nullptr) {
        return;
    }

    const auto kind = typeLayout->getKind();

    if (kind == slang::TypeReflection::Kind::ParameterBlock ||
        kind == slang::TypeReflection::Kind::ConstantBuffer) {
        // 中身(構造体)へ透過的に降りる。要素のvar layoutを経由することで、この
        // コンテナ自身が消費したbinding/spaceのオフセットが子の`getBindingSpace`/
        // `getOffset`に正しく反映される。
        slang::VariableLayoutReflection* elementVar = typeLayout->getElementVarLayout();
        if (elementVar != nullptr && elementVar != varLayout) {
            collectBindings(elementVar, name, out);
        }
        return;
    }

    if (kind == slang::TypeReflection::Kind::Struct) {
        const unsigned int fieldCount = typeLayout->getFieldCount();
        for (unsigned int i = 0; i < fieldCount; ++i) {
            collectBindings(typeLayout->getFieldByIndex(i), name, out);
        }
        return;
    }

    // ここに来るのはResource(Texture2D/RWTexture2D等)/SamplerState/
    // ShaderStorageBuffer(StructuredBuffer)のような「葉」ノード。
    const slang::ParameterCategory category = varLayout->getCategory();
    int shimKind = kSlangShimResourceUnknown;
    switch (category) {
        case slang::ParameterCategory::ShaderResource:
            shimKind = kSlangShimResourceShaderResource;
            break;
        case slang::ParameterCategory::UnorderedAccess:
            shimKind = kSlangShimResourceUnorderedAccess;
            break;
        case slang::ParameterCategory::SamplerState:
            shimKind = kSlangShimResourceSampler;
            break;
        case slang::ParameterCategory::ConstantBuffer:
            shimKind = kSlangShimResourceConstantBuffer;
            break;
        default:
            // 純粋な数値(uniform定数)フィールド等、bgfxのレジスタバインディング対象外。
            return;
    }

    SlangShimResourceBinding binding;
    binding.name = name;
    binding.category = shimKind;
    binding.space = static_cast<uint32_t>(varLayout->getBindingSpace((SlangParameterCategory)category));
    binding.index = static_cast<uint32_t>(varLayout->getOffset((SlangParameterCategory)category));

    // Sampler/ConstantBufferにはshape/access/elementTypeNameの概念が無い(Resource系の
    // ShaderResource/UnorderedAccessのみ意味を持つ)。`Texture2D<float4> tex[]`のような
    // 可変長配列宣言の場合、`getType()`自体はArray型を返すため`unwrapArray`相当で
    // 要素型まで潜る。
    if (shimKind == kSlangShimResourceShaderResource || shimKind == kSlangShimResourceUnorderedAccess) {
        slang::TypeReflection* fieldType = varLayout->getVariable() != nullptr ? varLayout->getVariable()->getType() : nullptr;
        while (fieldType != nullptr && fieldType->getKind() == slang::TypeReflection::Kind::Array) {
            binding.isArray = true;
            fieldType = fieldType->getElementType();
        }
        if (fieldType != nullptr) {
            switch (fieldType->getResourceShape() & SLANG_RESOURCE_BASE_SHAPE_MASK) {
                case SLANG_TEXTURE_2D:
                    binding.shape = kSlangShimShapeTexture2D;
                    break;
                case SLANG_STRUCTURED_BUFFER:
                    binding.shape = kSlangShimShapeStructuredBuffer;
                    break;
                default:
                    binding.shape = kSlangShimShapeUnknown;
                    break;
            }
            binding.access = fieldType->getResourceAccess() == SLANG_RESOURCE_ACCESS_READ_WRITE
                                  ? kSlangShimAccessReadWrite
                                  : kSlangShimAccessRead;
            if (binding.shape == kSlangShimShapeStructuredBuffer) {
                slang::TypeReflection* elementType = fieldType->getResourceResultType();
                if (elementType != nullptr && elementType->getName() != nullptr) {
                    binding.elementTypeName = elementType->getName();
                }
            }
        }
    }

    out.push_back(std::move(binding));
}

}  // namespace

SlangShimDxilCompileResult* slang_shim_compile_to_dxil(
    const char* module_name,
    const char* module_path,
    const char* source,
    const char* entry_point_name,
    const char* const* search_paths,
    size_t search_path_count,
    const char* const* define_names,
    const char* const* define_values,
    size_t define_count) {
    auto* result = new SlangShimDxilCompileResult();

    slang::IGlobalSession* globalSession = global_session();
    if (globalSession == nullptr) {
        result->diagnostics = "Failed to create Slang global session";
        return result;
    }

    slang::TargetDesc targetDesc = {};
    targetDesc.format = SLANG_DXIL;
    targetDesc.profile = globalSession->findProfile("cs_6_0");

    std::vector<slang::PreprocessorMacroDesc> macros;
    macros.reserve(define_count);
    for (size_t i = 0; i < define_count; ++i) {
        macros.push_back({define_names[i], define_values[i]});
    }

    // 注意: Slangは`ParameterBlock<T>`をデフォルトでD3D12ターゲット向けに個別の
    // register spaceへ割り当てる(実測確認済み: `res`/`inputs`のように複数の
    // ParameterBlockがあると2つ目以降がspace 1,2,...になる)。bgfxのD3D12
    // ルートシグネチャはregister space 0のみを対象にするため、複数の
    // ParameterBlockを1つに合体してから渡す必要がある
    // (`merge_input_and_res_parameter_blocks`、呼び出し側の`slang_compiler`参照)。
    // Slangのコンパイラオプション(`ParameterBlocksUseRegisterSpaces`等)による
    // 抑制も試したが効果が無かった(deprecatedで機能していない)。

    slang::SessionDesc sessionDesc = {};
    sessionDesc.targets = &targetDesc;
    sessionDesc.targetCount = 1;
    sessionDesc.searchPaths = search_paths;
    sessionDesc.searchPathCount = static_cast<SlangInt>(search_path_count);
    sessionDesc.preprocessorMacros = macros.empty() ? nullptr : macros.data();
    sessionDesc.preprocessorMacroCount = static_cast<SlangInt>(macros.size());

    Slang::ComPtr<slang::ISession> session;
    if (SLANG_FAILED(globalSession->createSession(sessionDesc, session.writeRef()))) {
        result->diagnostics = "Failed to create Slang compile session";
        return result;
    }

    Slang::ComPtr<slang::IModule> module;
    {
        Slang::ComPtr<slang::IBlob> diagnosticsBlob;
        module = session->loadModuleFromSourceString(module_name, module_path, source,
                                                       diagnosticsBlob.writeRef());
        append_diagnostics(result->diagnostics, diagnosticsBlob);
    }
    if (module == nullptr) {
        return result;
    }

    Slang::ComPtr<slang::IEntryPoint> entryPoint;
    module->findEntryPointByName(entry_point_name, entryPoint.writeRef());
    if (entryPoint == nullptr) {
        result->diagnostics += "\nEntry point not found: ";
        result->diagnostics += entry_point_name;
        return result;
    }

    Slang::ComPtr<slang::IComponentType> composedProgram;
    {
        slang::IComponentType* components[] = {module, entryPoint};
        Slang::ComPtr<slang::IBlob> diagnosticsBlob;
        SlangResult compositeResult = session->createCompositeComponentType(
            components, 2, composedProgram.writeRef(), diagnosticsBlob.writeRef());
        append_diagnostics(result->diagnostics, diagnosticsBlob);
        if (SLANG_FAILED(compositeResult)) {
            return result;
        }
    }

    Slang::ComPtr<slang::IComponentType> linkedProgram;
    {
        Slang::ComPtr<slang::IBlob> diagnosticsBlob;
        SlangResult linkResult =
            composedProgram->link(linkedProgram.writeRef(), diagnosticsBlob.writeRef());
        append_diagnostics(result->diagnostics, diagnosticsBlob);
        if (SLANG_FAILED(linkResult)) {
            return result;
        }
    }

    Slang::ComPtr<slang::IBlob> dxilBlob;
    {
        Slang::ComPtr<slang::IBlob> diagnosticsBlob;
        SlangResult codeResult = linkedProgram->getEntryPointCode(
            0, 0, dxilBlob.writeRef(), diagnosticsBlob.writeRef());
        append_diagnostics(result->diagnostics, diagnosticsBlob);
        if (SLANG_FAILED(codeResult) || dxilBlob == nullptr) {
            return result;
        }
    }

    // トップレベルのグローバルシェーダーパラメータ(`inputs`/`res`等)からリソース
    // バインディングを再帰的に収集する。
    slang::ProgramLayout* programLayout = linkedProgram->getLayout();
    if (programLayout != nullptr) {
        const unsigned int paramCount = programLayout->getParameterCount();
        for (unsigned int i = 0; i < paramCount; ++i) {
            collectBindings(programLayout->getParameterByIndex(i), "", result->bindings);
        }
    }

    const size_t byteSize = dxilBlob->getBufferSize();
    result->dxil.resize(byteSize);
    std::memcpy(result->dxil.data(), dxilBlob->getBufferPointer(), byteSize);
    result->success = true;

    return result;
}

int slang_shim_dxil_result_success(const SlangShimDxilCompileResult* result) {
    return result->success ? 1 : 0;
}

const uint8_t* slang_shim_dxil_result_data(const SlangShimDxilCompileResult* result) {
    return result->success ? result->dxil.data() : nullptr;
}

size_t slang_shim_dxil_result_byte_count(const SlangShimDxilCompileResult* result) {
    return result->success ? result->dxil.size() : 0;
}

const char* slang_shim_dxil_result_diagnostics(const SlangShimDxilCompileResult* result) {
    return result->diagnostics.c_str();
}

size_t slang_shim_dxil_result_binding_count(const SlangShimDxilCompileResult* result) {
    return result->bindings.size();
}

const char* slang_shim_dxil_result_binding_name(const SlangShimDxilCompileResult* result, size_t index) {
    return result->bindings[index].name.c_str();
}

int slang_shim_dxil_result_binding_category(const SlangShimDxilCompileResult* result, size_t index) {
    return result->bindings[index].category;
}

uint32_t slang_shim_dxil_result_binding_space(const SlangShimDxilCompileResult* result, size_t index) {
    return result->bindings[index].space;
}

uint32_t slang_shim_dxil_result_binding_register(const SlangShimDxilCompileResult* result, size_t index) {
    return result->bindings[index].index;
}

int slang_shim_dxil_result_binding_shape(const SlangShimDxilCompileResult* result, size_t index) {
    return result->bindings[index].shape;
}

int slang_shim_dxil_result_binding_access(const SlangShimDxilCompileResult* result, size_t index) {
    return result->bindings[index].access;
}

const char* slang_shim_dxil_result_binding_element_type_name(const SlangShimDxilCompileResult* result, size_t index) {
    return result->bindings[index].elementTypeName.c_str();
}

int slang_shim_dxil_result_binding_is_array(const SlangShimDxilCompileResult* result, size_t index) {
    return result->bindings[index].isArray ? 1 : 0;
}

void slang_shim_dxil_result_free(SlangShimDxilCompileResult* result) { delete result; }

int slang_shim_result_success(const SlangShimCompileResult* result) {
    return result->success ? 1 : 0;
}

const uint32_t* slang_shim_result_spirv_data(const SlangShimCompileResult* result) {
    return result->success ? result->spirv.data() : nullptr;
}

size_t slang_shim_result_spirv_word_count(const SlangShimCompileResult* result) {
    return result->success ? result->spirv.size() : 0;
}

const char* slang_shim_result_diagnostics(const SlangShimCompileResult* result) {
    return result->diagnostics.c_str();
}

void slang_shim_result_free(SlangShimCompileResult* result) { delete result; }

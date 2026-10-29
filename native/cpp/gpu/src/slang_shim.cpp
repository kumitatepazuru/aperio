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

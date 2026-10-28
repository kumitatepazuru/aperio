#include "hlsl_shim.h"

#ifdef _WIN32
#include <windows.h>
#endif
#include <dxcapi.h>

#include <cstring>
#include <string>
#include <vector>

struct HlslShimCompileResult {
    bool success = false;
    std::vector<uint32_t> spirv;
    std::string diagnostics;
};

namespace {

// DXCの引数はワイド文字列。入力はUTF-8。
std::wstring to_wide(const char* utf8) {
    std::wstring out;
    if (utf8 == nullptr) {
        return out;
    }
#ifdef _WIN32
    int len = MultiByteToWideChar(CP_UTF8, 0, utf8, -1, nullptr, 0);
    if (len > 1) {
        out.resize(static_cast<size_t>(len) - 1);
        MultiByteToWideChar(CP_UTF8, 0, utf8, -1, out.data(), len - 1);
    }
#else
    // wchar_tが32bitの環境ではコードポイントをそのまま入れればよい。
    const unsigned char* p = reinterpret_cast<const unsigned char*>(utf8);
    while (*p) {
        uint32_t cp;
        int extra;
        if (*p < 0x80) { cp = *p; extra = 0; }
        else if ((*p >> 5) == 0x6) { cp = *p & 0x1f; extra = 1; }
        else if ((*p >> 4) == 0xe) { cp = *p & 0x0f; extra = 2; }
        else { cp = *p & 0x07; extra = 3; }
        ++p;
        for (int i = 0; i < extra && *p; ++i, ++p) {
            cp = (cp << 6) | (*p & 0x3f);
        }
        out.push_back(static_cast<wchar_t>(cp));
    }
#endif
    return out;
}

template <typename T>
void release(T*& p) {
    if (p != nullptr) {
        p->Release();
        p = nullptr;
    }
}

}  // namespace

HlslShimCompileResult* hlsl_shim_compile_to_spirv(
    const char* source_name,
    const char* source,
    const char* entry_point_name,
    const char* target_profile,
    const char* const* include_dirs,
    size_t include_dir_count,
    const char* const* define_names,
    const char* const* define_values,
    size_t define_count) {
    auto* result = new HlslShimCompileResult();

    IDxcUtils* utils = nullptr;
    IDxcCompiler3* compiler = nullptr;
    IDxcIncludeHandler* include_handler = nullptr;
    IDxcResult* dxc_result = nullptr;
    IDxcBlobUtf8* errors = nullptr;
    IDxcBlob* object = nullptr;

    if (FAILED(DxcCreateInstance(CLSID_DxcUtils, IID_PPV_ARGS(&utils))) ||
        FAILED(DxcCreateInstance(CLSID_DxcCompiler, IID_PPV_ARGS(&compiler)))) {
        result->diagnostics = "Failed to create the DXC compiler (is dxcompiler available at runtime?)";
        release(utils);
        release(compiler);
        return result;
    }
    utils->CreateDefaultIncludeHandler(&include_handler);

    // 引数の文字列はCompile呼び出しの間生きている必要があるので、先に全部ワイド文字列として確保する。
    std::vector<std::wstring> args;
    args.push_back(L"-E");
    args.push_back(to_wide(entry_point_name));
    args.push_back(L"-T");
    args.push_back(to_wide(target_profile));
    args.push_back(L"-spirv");
    args.push_back(L"-fspv-target-env=vulkan1.2");
    for (size_t i = 0; i < include_dir_count; ++i) {
        args.push_back(L"-I");
        args.push_back(to_wide(include_dirs[i]));
    }
    for (size_t i = 0; i < define_count; ++i) {
        std::wstring def = to_wide(define_names[i]);
        def += L"=";
        def += to_wide(define_values[i]);
        args.push_back(L"-D");
        args.push_back(def);
    }
    if (source_name != nullptr) {
        args.push_back(to_wide(source_name));
    }
    std::vector<LPCWSTR> arg_ptrs;
    for (const auto& a : args) {
        arg_ptrs.push_back(a.c_str());
    }

    DxcBuffer buffer;
    buffer.Ptr = source;
    buffer.Size = std::strlen(source);
    buffer.Encoding = DXC_CP_UTF8;

    HRESULT hr = compiler->Compile(
        &buffer, arg_ptrs.data(), static_cast<UINT32>(arg_ptrs.size()), include_handler,
        IID_PPV_ARGS(&dxc_result));
    if (FAILED(hr) || dxc_result == nullptr) {
        result->diagnostics = "IDxcCompiler3::Compile failed";
    } else {
        if (SUCCEEDED(dxc_result->GetOutput(DXC_OUT_ERRORS, IID_PPV_ARGS(&errors), nullptr)) &&
            errors != nullptr && errors->GetStringLength() > 0) {
            result->diagnostics.assign(errors->GetStringPointer(), errors->GetStringLength());
        }
        HRESULT status = S_OK;
        dxc_result->GetStatus(&status);
        if (SUCCEEDED(status) &&
            SUCCEEDED(dxc_result->GetOutput(DXC_OUT_OBJECT, IID_PPV_ARGS(&object), nullptr)) &&
            object != nullptr && object->GetBufferSize() >= sizeof(uint32_t)) {
            const size_t words = object->GetBufferSize() / sizeof(uint32_t);
            result->spirv.resize(words);
            std::memcpy(result->spirv.data(), object->GetBufferPointer(), words * sizeof(uint32_t));
            result->success = true;
        }
    }

    release(object);
    release(errors);
    release(dxc_result);
    release(include_handler);
    release(compiler);
    release(utils);
    return result;
}

int hlsl_shim_result_success(const HlslShimCompileResult* result) {
    return result != nullptr && result->success ? 1 : 0;
}

const uint32_t* hlsl_shim_result_spirv_data(const HlslShimCompileResult* result) {
    return (result != nullptr && result->success) ? result->spirv.data() : nullptr;
}

size_t hlsl_shim_result_spirv_word_count(const HlslShimCompileResult* result) {
    return (result != nullptr && result->success) ? result->spirv.size() : 0;
}

const char* hlsl_shim_result_diagnostics(const HlslShimCompileResult* result) {
    return result != nullptr ? result->diagnostics.c_str() : "";
}

void hlsl_shim_result_free(HlslShimCompileResult* result) {
    delete result;
}

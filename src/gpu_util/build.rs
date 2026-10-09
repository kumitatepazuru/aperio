use std::env;
use std::path::PathBuf;

fn main() {
    let triplet = env::var("VCPKG_DEFAULT_TRIPLET").unwrap_or_else(|_| {
        if cfg!(target_os = "windows") {
            if cfg!(target_arch = "aarch64") {
                "arm64-windows-static-md".to_string()
            } else {
                "x64-windows-static-md".to_string()
            }
        } else if cfg!(target_os = "linux") {
            if cfg!(target_arch = "aarch64") {
                "arm64-linux".to_string()
            } else {
                "x64-linux".to_string()
            }
        } else {
            "aarch64-osx".to_string() // macOSはarm64が主流なので、x64はサポート外とする
        }
    });

    let vcpkg_installed = format!(
        "{}/./cpp/vcpkg_installed/{}",
        env!("CARGO_MANIFEST_DIR"),
        triplet
    );
    let slang_include = format!("{}/include", vcpkg_installed);
    let slang_lib = format!("{}/lib", vcpkg_installed);
    let slang_bin = format!("{}/bin", vcpkg_installed);
    let dxc_include = format!("{}/include/directx-dxc", vcpkg_installed);

    // ── compile C++ shim ────────────────────────────────────────────────────
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .file("./cpp/src/slang_shim.cpp")
        .include("./cpp/include")
        .include(&slang_include)
        // MSVC: enable exceptions, suppress deprecation warnings from vendor headers.
        // ソースにUTF-8の日本語コメントを含むため、/utf-8を指定しないと非UTF-8ロケール
        // (例: 日本語版Windowsの既定コードページ)環境で文字化けによる構文エラーになる。
        .flag_if_supported("/utf-8")
        .flag_if_supported("/EHsc")
        .flag_if_supported("/wd4244")
        .flag_if_supported("/wd4267")
        // GCC/Clang
        .flag_if_supported("-Wno-deprecated-declarations")
        .compile("gpu_util_slang_shim");

    // ── compile C++ shim (HLSL / DXC) ───────────────────────────────────────
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .file("./cpp/src/hlsl_shim.cpp")
        .include("./cpp/include")
        .include(&dxc_include)
        .flag_if_supported("/utf-8")
        .flag_if_supported("/EHsc")
        .flag_if_supported("/wd4244")
        .flag_if_supported("/wd4267")
        .flag_if_supported("-Wno-deprecated-declarations")
        .compile("gpu_util_hlsl_shim");

    // ── link Slang ──────────────────────────────────────────────────────────
    // shader-slangはtripletのstatic/static-md設定に関わらず、本体(コード生成バックエンド
    // 含む)が常に共有ライブラリ(Windows: .dll / Linux: .so / macOS: .dylib)として配布される
    // (アップストリームのCMake構成がコア部分を常に共有ライブラリとしてビルドするため。
    // static-mdはvcpkgのpost-buildポリシー上「staticトリプレットは共有ライブラリを
    // 生成してはならない」制約を回避するために選んでいるだけで、Slang自体を静的リンクに
    // できるわけではない)。そのためリンクはインポートライブラリ/リンク用スタブに対して行い、
    // 実行時には本体の共有ライブラリを別途配置する必要がある(下のcopy_runtime_shared_libs参照)。
    // 対照的に、`avloader`が使うffmpeg等は通常のvcpkgポート同様tripletの静的リンク設定に
    // 従うため、この対応は不要。
    println!("cargo:rustc-link-search=native={}", slang_lib);
    println!("cargo:rustc-link-lib=slang");

    // ── Slangの実行時共有ライブラリを配置 ────────────────────────────────────────
    // `slang`本体が内部のコード生成バックエンド(LLVM JIT/GLSL変換等)を追加の共有ライブラリ
    // として実行時に読み込むため、リンクだけでは足りずビルド成果物と同じディレクトリに
    // これらを配置しておく必要がある(gfxはSlang付属のサンプル用レンダリング抽象化層で
    // このシムでは使わないため対象外)。ELF/Mach-Oは実行体自身のディレクトリを自動探索
    // しないため、Linux/macOSではrpathも合わせて設定する。
    let slang_runtime_libs = [
        "slang",
        "slang-compiler",
        "slang-glsl-module",
        "slang-glslang",
        "slang-llvm",
        "slang-rt",
    ];
    if cfg!(target_os = "windows") {
        let files: Vec<String> = slang_runtime_libs.iter().map(|n| format!("{n}.dll")).collect();
        copy_runtime_shared_libs(&slang_bin, &files);
    } else if cfg!(target_os = "linux") {
        let files: Vec<String> = slang_runtime_libs.iter().map(|n| format!("lib{n}.so")).collect();
        copy_runtime_shared_libs(&slang_lib, &files);
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
    } else if cfg!(target_os = "macos") {
        let files: Vec<String> = slang_runtime_libs.iter().map(|n| format!("lib{n}.dylib")).collect();
        copy_runtime_shared_libs(&slang_lib, &files);
        // `gpu_util`は最終的にcdylibを生成する`native`クレートから使われるため、
        // 実行体基準の@executable_pathより読み込み元基準の@loader_pathの方が適切。
        println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");
    }

    // ── Link DXC (HLSL → SPIR-V) ────────────────────────────────────────────
    // dxcompilerもSlangと同様に共有ライブラリとして配布されるため、実行時に
    // 実行体と同じディレクトリへ配置する。(dxil.dllはDXIL署名用で、SPIR-V出力には不要。)
    println!("cargo:rustc-link-lib=dxcompiler");
    if cfg!(target_os = "windows") {
        copy_runtime_shared_libs(&slang_bin, &["dxcompiler.dll".to_string()]);
    } else if cfg!(target_os = "linux") {
        copy_runtime_shared_libs(&slang_lib, &["libdxcompiler.so".to_string()]);
    } else if cfg!(target_os = "macos") {
        copy_runtime_shared_libs(&slang_lib, &["libdxcompiler.dylib".to_string()]);
    }

    // ── C++ standard library (needed when linking C++ from Rust) ────────────
    if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-lib=stdc++");
    } else if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-lib=c++");
    }

    // ── bindgen (slang_shim) ──────────────────────────────────────────────
    let out_dir = env::var("OUT_DIR").unwrap();
    let header = "./cpp/include/slang_shim.h";

    let bindings = bindgen::Builder::default()
        .header(header)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings for slang_shim.h");

    bindings
        .write_to_file(PathBuf::from(&out_dir).join("slang_bindings.rs"))
        .expect("Couldn't write slang_bindings.rs");

    let hlsl_bindings = bindgen::Builder::default()
        .header("./cpp/include/hlsl_shim.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings for hlsl_shim.h");
    hlsl_bindings
        .write_to_file(PathBuf::from(&out_dir).join("hlsl_bindings.rs"))
        .expect("Couldn't write hlsl_bindings.rs");

    println!("cargo:rerun-if-changed=./cpp/include/slang_shim.h");
    println!("cargo:rerun-if-changed=./cpp/include/hlsl_shim.h");
    println!("cargo:rerun-if-changed=./cpp/src/hlsl_shim.cpp");
    println!("cargo:rerun-if-changed=./cpp/src/slang_shim.cpp");
}

/// `src_dir`内の`names`(ファイル名、拡張子込み)を、最終的な実行体が置かれる
/// `target/<profile>/`直下へコピーする。`OUT_DIR`は`target/<profile>/build/<pkg>-<hash>/out`
/// という形なので、3階層上が`target/<profile>/`になる。
fn copy_runtime_shared_libs(src_dir: &str, names: &[String]) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let Some(target_dir) = out_dir.ancestors().nth(3) else {
        return;
    };

    for name in names {
        let src = PathBuf::from(src_dir).join(name);
        let dst = target_dir.join(name);
        if src.exists() {
            let _ = std::fs::copy(&src, &dst);
        }
        println!("cargo:rerun-if-changed={}", src.display());
    }
}

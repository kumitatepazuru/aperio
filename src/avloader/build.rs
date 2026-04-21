use std::env;
use std::path::PathBuf;

fn main() {
    let triplet = env::var("VCPKG_DEFAULT_TRIPLET").unwrap_or_else(|_| {
        if cfg!(target_os = "windows") {
            if cfg!(target_arch = "aarch64") {
                "arm64-windows-static".to_string()
            } else {
                "x64-windows-static".to_string()
            }
        } else if cfg!(target_os = "linux") {
            if cfg!(target_arch = "aarch64") {
                "aarch64-linux".to_string()
            } else {
                "x64-linux".to_string()
            }
        } else {
            "aarch64-osx".to_string() // macOSはarm64が主流なので、x64はサポート外とする
        }
    });

    let vcpkg_installed = format!(
        "{}/../avloader-cpp/vcpkg_installed/{}",
        env!("CARGO_MANIFEST_DIR"),
        triplet
    );
    let ffmpeg_include = format!("{}/include", vcpkg_installed);
    let ffmpeg_lib = format!("{}/lib", vcpkg_installed);

    // ── compile C++ library ─────────────────────────────────────────────────
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .file("../avloader-cpp/src/video_decoder.cpp")
        .include("../avloader-cpp/include")
        .include(&ffmpeg_include)
        // MSVC: enable exceptions, suppress deprecation warnings from FFmpeg headers
        .flag_if_supported("/EHsc")
        .flag_if_supported("/wd4244")
        .flag_if_supported("/wd4267")
        // GCC/Clang
        .flag_if_supported("-Wno-deprecated-declarations")
        .compile("avloader_cpp");

    // ── link FFmpeg ─────────────────────────────────────────────────────────
    println!("cargo:rustc-link-search=native={}", ffmpeg_lib);
    println!("cargo:rustc-link-lib=avformat");
    println!("cargo:rustc-link-lib=avcodec");
    println!("cargo:rustc-link-lib=avutil");
    println!("cargo:rustc-link-lib=swscale");
    println!("cargo:rustc-link-lib=swresample");
    println!("cargo:rustc-link-lib=dav1d");
    println!("cargo:rustc-link-lib=libx264");
    println!("cargo:rustc-link-lib=x265-static");
    println!("cargo:rustc-link-lib=vpx");
    println!("cargo:rustc-link-lib=libmfx");
    println!("cargo:rustc-link-lib=vorbis");
    println!("cargo:rustc-link-lib=vorbisfile");
    println!("cargo:rustc-link-lib=vorbisenc");
    println!("cargo:rustc-link-lib=ogg");

    // Windows system libraries required by FFmpeg
    if cfg!(target_os = "windows") {
        for lib in &[
            "bcrypt", "ws2_32", "secur32", "mfplat", "mf", "mfuuid", "strmiids", "ole32", "user32",
        ] {
            println!("cargo:rustc-link-lib={}", lib);
        }
    }

    // ── C++ standard library (needed when linking C++ from Rust) ────────────
    if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-lib=stdc++");
    } else if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-lib=c++");
    }

    // ── bindgen ────────────────────────────────────────────────────────────
    let out_dir = env::var("OUT_DIR").unwrap();
    let header = "../avloader-cpp/include/avloader.h";

    let bindings = bindgen::Builder::default()
        .header(header)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings for avloader.h");

    bindings
        .write_to_file(PathBuf::from(&out_dir).join("avloader_bindings.rs"))
        .expect("Couldn't write avloader_bindings.rs");

    println!("cargo:rerun-if-changed=../avloader-cpp/include/avloader.h");
    println!("cargo:rerun-if-changed=../avloader-cpp/src/video_decoder.cpp");
}

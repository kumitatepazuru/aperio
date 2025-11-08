fn main() {
  // rpathを通す
    if cfg!(target_os = "linux") {
        // linuxの場合
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
    } else if cfg!(target_os = "macos") {
        // macosの場合
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
    }

  napi_build::setup();
}

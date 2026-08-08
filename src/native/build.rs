fn main() {
  napi_build::setup();

  if cfg!(target_os = "linux") {
    println!("cargo:rustc-cdylib-link-arg=-Wl,-rpath,$ORIGIN");
  } else if cfg!(target_os = "macos") {
    println!("cargo:rustc-cdylib-link-arg=-Wl,-rpath,@loader_path");
  }
}

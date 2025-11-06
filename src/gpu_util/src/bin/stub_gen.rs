use pyo3_stub_gen::Result;

fn main() -> Result<()> {
    // `stub_info` is a function defined by `define_stub_info_gatherer!` macro.
    println!("Generating stubs for gpu_util...");
    let stub = gpu_util::stub_info()?;
    stub.generate()?;
    Ok(())
}

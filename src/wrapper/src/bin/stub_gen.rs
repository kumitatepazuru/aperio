use pyo3_stub_gen::Result;

fn main() -> Result<()> {
    println!("Generating stubs for aperio wrapper...");
    let stub = wrapper::stub_info()?;
    stub.generate()?;
    Ok(())
}

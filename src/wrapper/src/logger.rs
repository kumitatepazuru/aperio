use log::{error, info, warn};
use pyo3::{prelude::*, types::PyModuleMethods};
use pyo3_stub_gen::derive::gen_stub_pyfunction;

#[gen_stub_pyfunction(module = "aperio.logger")]
#[pyfunction]
fn info(message: String) {
    info!("{}", message);
}

#[gen_stub_pyfunction(module = "aperio.logger")]
#[pyfunction]
fn warning(message: String) {
    warn!("{}", message);
}

#[gen_stub_pyfunction(module = "aperio.logger")]
#[pyfunction]
fn error(message: String) {
    error!("{}", message);
}

pub fn register(m: &Bound<PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(info, m)?)?;
    m.add_function(wrap_pyfunction!(warning, m)?)?;
    m.add_function(wrap_pyfunction!(error, m)?)?;
    Ok(())
}

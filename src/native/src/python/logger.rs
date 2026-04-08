use log::{error, info, warn};
use pyo3::{
    pyfunction, pymodule,
    types::{PyModule, PyModuleMethods},
    wrap_pyfunction, Bound, PyResult,
};

#[pyfunction]
fn info(message: String) {
    info!("{}", message);
}

#[pyfunction]
fn warning(message: String) {
    warn!("{}", message);
}

#[pyfunction]
fn error(message: String) {
    error!("{}", message);
}

#[pymodule]
pub fn logger(m: &Bound<PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(info, m)?)?;
    m.add_function(wrap_pyfunction!(warning, m)?)?;
    m.add_function(wrap_pyfunction!(error, m)?)?;

    Ok(())
}

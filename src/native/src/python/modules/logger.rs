use log::{error, info, warn};
use pyo3::prelude::*;

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
pub mod logger {
    #[pymodule_export]
    use super::error;
    #[pymodule_export]
    use super::info;
    #[pymodule_export]
    use super::warning;
}

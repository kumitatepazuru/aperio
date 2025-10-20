use std::convert::Infallible;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AperioError {
    #[error("Serialization Json Error: {0}")]
    SerdeJsonError(#[from] serde_json::Error),

    #[error("IO Error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Infallible: {0}")]
    Infallible(#[from] Infallible),
    
    #[error("File Not Found: {0}")]
    FileNotFound(String),
}

pub type AperioResult<T> = Result<T, AperioError>;

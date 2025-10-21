use std::convert::Infallible;
use std::string::FromUtf8Error;
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
    
    #[error("Command failed with status: {0}")]
    CommandFailed(String),
    
    #[error("UTF-8 Error: {0}")]
    Utf8Error(#[from] FromUtf8Error),
}

pub type AperioResult<T> = Result<T, AperioError>;

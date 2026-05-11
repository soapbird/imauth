use thiserror::Error;

#[derive(Error, Debug)]
pub enum ImauthError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Encryption error: {0}")]
    Encryption(String),

    #[error("Browser error: {0}")]
    Browser(String),

    #[error("Platform error: {0}")]
    Platform(String),

    #[error("Queue error: {0}")]
    Queue(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

impl From<sqlx::Error> for ImauthError {
    fn from(e: sqlx::Error) -> Self {
        ImauthError::Database(e.to_string())
    }
}

impl From<aes_gcm::Error> for ImauthError {
    fn from(e: aes_gcm::Error) -> Self {
        ImauthError::Encryption(e.to_string())
    }
}

impl From<base64::DecodeError> for ImauthError {
    fn from(e: base64::DecodeError) -> Self {
        ImauthError::Encryption(format!("Invalid base64: {e}"))
    }
}

pub type Result<T> = std::result::Result<T, ImauthError>;

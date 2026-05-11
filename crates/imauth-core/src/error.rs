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

    #[error("Session error: {0}")]
    Session(String),

    #[error("Credential error: {0}")]
    Credential(String),

    #[error("Queue error: {0}")]
    Queue(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, ImauthError>;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DomainError {
    #[error("Configuration error: {0}")]
    ConfigError(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
}

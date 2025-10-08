//! Error types for the OpenFAN system

use thiserror::Error;

/// Core error type for OpenFAN operations
#[derive(Error, Debug)]
pub enum OpenFanError {
    /// Configuration-related errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// Hardware communication errors
    #[error("Hardware error: {0}")]
    Hardware(String),

    /// Serial port errors
    #[error("Serial port error: {0}")]
    Serial(String),

    /// Invalid input or arguments
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Profile not found
    #[error("Profile not found: {0}")]
    ProfileNotFound(String),

    /// Alias not found
    #[error("Alias not found for fan {0}")]
    AliasNotFound(u8),

    /// Fan ID out of range
    #[error("Fan ID out of range: {0} (must be 0-9)")]
    InvalidFanId(u8),

    /// Parsing errors
    #[error("Parse error: {0}")]
    Parse(String),

    /// I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization/deserialization errors
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Timeout errors
    #[error("Operation timed out: {0}")]
    Timeout(String),

    /// Device not found
    #[error("Device not found")]
    DeviceNotFound,

    /// Generic error
    #[error("{0}")]
    Other(String),
}

/// Result type alias for OpenFAN operations
pub type Result<T> = std::result::Result<T, OpenFanError>;

impl From<serde_json::Error> for OpenFanError {
    fn from(err: serde_json::Error) -> Self {
        OpenFanError::Serialization(err.to_string())
    }
}

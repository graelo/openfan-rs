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

    /// Zone not found
    #[error("Zone not found: {0}")]
    ZoneNotFound(String),

    /// Thermal curve not found
    #[error("Thermal curve not found: {0}")]
    CurveNotFound(String),

    /// CFM mapping not found
    #[error("CFM mapping not found for port {0}")]
    CfmMappingNotFound(u8),

    /// Fan ID out of range
    #[error("Fan ID out of range: {fan_id} (must be 0-{max})", max = max_fans - 1)]
    InvalidFanId { fan_id: u8, max_fans: usize },

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

    /// Device disconnected (USB unplugged, power cycle)
    #[error("Device disconnected: {0}")]
    DeviceDisconnected(String),

    /// Reconnection in progress
    #[error("Reconnection in progress")]
    Reconnecting,

    /// Reconnection failed after max retries
    #[error("Reconnection failed after {attempts} attempts: {reason}")]
    ReconnectionFailed { attempts: u32, reason: String },

    /// Controller not found
    #[error("Controller not found: {0}")]
    ControllerNotFound(String),

    /// Controller ID required but not provided
    #[error("Controller ID required")]
    ControllerIdRequired,

    /// Duplicate controller ID in configuration
    #[error("Duplicate controller ID: {0}")]
    DuplicateControllerId(String),

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serde_json_error_conversion() {
        // Create a serde_json error by trying to parse invalid JSON
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let openfan_err: OpenFanError = json_err.into();

        match openfan_err {
            OpenFanError::Serialization(msg) => {
                assert!(!msg.is_empty());
            }
            _ => panic!("Expected Serialization error"),
        }
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let openfan_err: OpenFanError = io_err.into();

        match openfan_err {
            OpenFanError::Io(e) => {
                assert_eq!(e.kind(), std::io::ErrorKind::NotFound);
            }
            _ => panic!("Expected Io error"),
        }
    }

    #[test]
    fn test_error_display() {
        // Test Display implementation for various error types
        let err = OpenFanError::Config("test config error".to_string());
        assert_eq!(format!("{}", err), "Configuration error: test config error");

        let err = OpenFanError::Hardware("test hardware error".to_string());
        assert_eq!(format!("{}", err), "Hardware error: test hardware error");

        let err = OpenFanError::InvalidFanId {
            fan_id: 15,
            max_fans: 10,
        };
        assert_eq!(format!("{}", err), "Fan ID out of range: 15 (must be 0-9)");

        let err = OpenFanError::DeviceNotFound;
        assert_eq!(format!("{}", err), "Device not found");

        let err = OpenFanError::Reconnecting;
        assert_eq!(format!("{}", err), "Reconnection in progress");

        let err = OpenFanError::ReconnectionFailed {
            attempts: 5,
            reason: "timeout".to_string(),
        };
        assert_eq!(
            format!("{}", err),
            "Reconnection failed after 5 attempts: timeout"
        );
    }
}

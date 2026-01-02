//! Static configuration loaded once at startup
//!
//! This configuration is read-only after the daemon starts.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::paths::default_data_dir;

// Default value helpers for serde
fn default_true() -> bool {
    true
}
fn default_one() -> u64 {
    1
}
fn default_thirty() -> u64 {
    30
}
fn default_two() -> f64 {
    2.0
}
fn default_ten() -> u64 {
    10
}

/// Reconnection configuration for device disconnect handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectConfig {
    /// Enable automatic reconnection (default: true)
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Maximum reconnection attempts (0 = unlimited, default: 0)
    #[serde(default)]
    pub max_attempts: u32,

    /// Initial delay in seconds before first reconnection attempt (default: 1)
    #[serde(default = "default_one")]
    pub initial_delay_secs: u64,

    /// Maximum delay in seconds between reconnection attempts (default: 30)
    #[serde(default = "default_thirty")]
    pub max_delay_secs: u64,

    /// Backoff multiplier for exponential backoff (default: 2.0)
    #[serde(default = "default_two")]
    pub backoff_multiplier: f64,

    /// Enable background heartbeat for connection monitoring (default: true)
    #[serde(default = "default_true")]
    pub enable_heartbeat: bool,

    /// Heartbeat interval in seconds (default: 10)
    #[serde(default = "default_ten")]
    pub heartbeat_interval_secs: u64,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: 0,
            initial_delay_secs: 1,
            max_delay_secs: 30,
            backoff_multiplier: 2.0,
            enable_heartbeat: true,
            heartbeat_interval_secs: 10,
        }
    }
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Address to bind to (e.g., "127.0.0.1" or "0.0.0.0" for all interfaces)
    pub bind_address: String,
    /// Server port
    pub port: u16,
    /// Communication timeout in seconds
    pub communication_timeout: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".to_string(),
            port: 3000,
            communication_timeout: 1,
        }
    }
}

/// Static configuration for the OpenFAN daemon.
///
/// This is loaded once at startup and remains immutable during runtime.
/// Located at `~/.config/openfan/config.toml` by default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticConfig {
    /// Server configuration (bind address, port, timeout)
    #[serde(default)]
    pub server: ServerConfig,

    /// Directory for mutable data files (aliases, profiles, etc.)
    ///
    /// Defaults to `~/.local/share/openfan` (XDG data directory).
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,

    /// Reconnection configuration for device disconnect handling
    #[serde(default)]
    pub reconnect: ReconnectConfig,
}

impl Default for StaticConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            data_dir: default_data_dir(),
            reconnect: ReconnectConfig::default(),
        }
    }
}

impl StaticConfig {
    /// Create a new StaticConfig with a custom data directory.
    pub fn with_data_dir(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            ..Default::default()
        }
    }

    /// Parse StaticConfig from TOML string.
    pub fn from_toml(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }

    /// Serialize StaticConfig to TOML string.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_static_config() {
        let config = StaticConfig::default();
        assert_eq!(config.server.port, 3000);
        assert_eq!(config.server.bind_address, "127.0.0.1");
    }

    #[test]
    fn test_static_config_serialization() {
        let config = StaticConfig::default();
        let toml_str = config.to_toml().unwrap();

        assert!(toml_str.contains("[server]"));
        assert!(toml_str.contains("data_dir"));
    }

    #[test]
    fn test_static_config_deserialization() {
        let toml_str = r#"
            data_dir = "/custom/data"

            [server]
            bind_address = "0.0.0.0"
            port = 8080
            communication_timeout = 5
        "#;

        let config = StaticConfig::from_toml(toml_str).unwrap();
        assert_eq!(config.server.bind_address, "0.0.0.0");
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.data_dir, PathBuf::from("/custom/data"));
    }

    #[test]
    fn test_static_config_default_data_dir() {
        // When data_dir is not specified, it should use the default
        let toml_str = r#"
            [server]
            bind_address = "127.0.0.1"
            port = 3000
            communication_timeout = 1
        "#;

        let config = StaticConfig::from_toml(toml_str).unwrap();
        assert!(config.data_dir.ends_with("openfan"));
    }

    #[test]
    fn test_static_config_minimal() {
        // Minimal config - only data_dir, server uses defaults
        let toml_str = r#"
            data_dir = "/var/lib/openfan"
        "#;

        let config = StaticConfig::from_toml(toml_str).unwrap();
        assert_eq!(config.server.bind_address, "127.0.0.1");
        assert_eq!(config.server.port, 3000);
        assert_eq!(config.data_dir, PathBuf::from("/var/lib/openfan"));
    }

    #[test]
    fn test_static_config_empty() {
        // Empty config - all defaults
        let toml_str = "";

        let config = StaticConfig::from_toml(toml_str).unwrap();
        assert_eq!(config.server.bind_address, "127.0.0.1");
        assert_eq!(config.server.port, 3000);
        assert!(config.data_dir.ends_with("openfan"));
    }

    #[test]
    fn test_reconnect_config_defaults() {
        let config = ReconnectConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_attempts, 0);
        assert_eq!(config.initial_delay_secs, 1);
        assert_eq!(config.max_delay_secs, 30);
        assert!((config.backoff_multiplier - 2.0).abs() < f64::EPSILON);
        assert!(config.enable_heartbeat);
        assert_eq!(config.heartbeat_interval_secs, 10);
    }

    #[test]
    fn test_static_config_with_reconnect_section() {
        let toml_str = r#"
            data_dir = "/var/lib/openfan"

            [server]
            bind_address = "127.0.0.1"
            port = 3000
            communication_timeout = 1

            [reconnect]
            enabled = true
            max_attempts = 5
            initial_delay_secs = 2
            max_delay_secs = 60
            backoff_multiplier = 1.5
            enable_heartbeat = false
            heartbeat_interval_secs = 30
        "#;

        let config = StaticConfig::from_toml(toml_str).unwrap();
        assert!(config.reconnect.enabled);
        assert_eq!(config.reconnect.max_attempts, 5);
        assert_eq!(config.reconnect.initial_delay_secs, 2);
        assert_eq!(config.reconnect.max_delay_secs, 60);
        assert!((config.reconnect.backoff_multiplier - 1.5).abs() < f64::EPSILON);
        assert!(!config.reconnect.enable_heartbeat);
        assert_eq!(config.reconnect.heartbeat_interval_secs, 30);
    }

    #[test]
    fn test_static_config_reconnect_defaults_when_missing() {
        // When reconnect section is missing, use defaults
        let toml_str = r#"
            data_dir = "/var/lib/openfan"
        "#;

        let config = StaticConfig::from_toml(toml_str).unwrap();
        assert!(config.reconnect.enabled);
        assert_eq!(config.reconnect.max_attempts, 0);
        assert_eq!(config.reconnect.initial_delay_secs, 1);
    }

    #[test]
    fn test_static_config_reconnect_partial() {
        // Partial reconnect section - missing fields use defaults
        let toml_str = r#"
            [reconnect]
            max_attempts = 10
        "#;

        let config = StaticConfig::from_toml(toml_str).unwrap();
        assert!(config.reconnect.enabled); // default
        assert_eq!(config.reconnect.max_attempts, 10);
        assert_eq!(config.reconnect.initial_delay_secs, 1); // default
    }

    #[test]
    fn test_static_config_reconnect_disabled() {
        let toml_str = r#"
            [reconnect]
            enabled = false
        "#;

        let config = StaticConfig::from_toml(toml_str).unwrap();
        assert!(!config.reconnect.enabled);
    }
}

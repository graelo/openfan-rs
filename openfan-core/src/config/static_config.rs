//! Static configuration loaded once at startup
//!
//! This configuration is read-only after the daemon starts.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::paths::default_data_dir;

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server hostname
    pub hostname: String,
    /// Server port
    pub port: u16,
    /// Communication timeout in seconds
    pub communication_timeout: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            hostname: "localhost".to_string(),
            port: 3000,
            communication_timeout: 1,
        }
    }
}

/// Hardware configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareConfig {
    /// Hardware hostname (for network-based hardware)
    pub hostname: String,
    /// Hardware port
    pub port: u16,
    /// Communication timeout in seconds
    pub communication_timeout: u64,
}

impl Default for HardwareConfig {
    fn default() -> Self {
        Self {
            hostname: "localhost".to_string(),
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
    /// Server configuration (hostname, port, timeout)
    pub server: ServerConfig,

    /// Hardware configuration (hostname, port, timeout)
    pub hardware: HardwareConfig,

    /// Directory for mutable data files (aliases, profiles, etc.)
    ///
    /// Defaults to `~/.local/share/openfan` (XDG data directory).
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
}

impl Default for StaticConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            hardware: HardwareConfig::default(),
            data_dir: default_data_dir(),
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
        assert_eq!(config.server.hostname, "localhost");
    }

    #[test]
    fn test_static_config_serialization() {
        let config = StaticConfig::default();
        let toml_str = config.to_toml().unwrap();

        assert!(toml_str.contains("[server]"));
        assert!(toml_str.contains("[hardware]"));
        assert!(toml_str.contains("data_dir"));
    }

    #[test]
    fn test_static_config_deserialization() {
        let toml_str = r#"
            data_dir = "/custom/data"

            [server]
            hostname = "0.0.0.0"
            port = 8080
            communication_timeout = 5

            [hardware]
            hostname = "192.168.1.100"
            port = 3001
            communication_timeout = 2
        "#;

        let config = StaticConfig::from_toml(toml_str).unwrap();
        assert_eq!(config.server.hostname, "0.0.0.0");
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.hardware.hostname, "192.168.1.100");
        assert_eq!(config.data_dir, PathBuf::from("/custom/data"));
    }

    #[test]
    fn test_static_config_default_data_dir() {
        // When data_dir is not specified, it should use the default
        let toml_str = r#"
            [server]
            hostname = "localhost"
            port = 3000
            communication_timeout = 1

            [hardware]
            hostname = "localhost"
            port = 3000
            communication_timeout = 1
        "#;

        let config = StaticConfig::from_toml(toml_str).unwrap();
        assert!(config.data_dir.ends_with("openfan"));
    }
}

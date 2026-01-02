//! Static configuration loaded once at startup
//!
//! This configuration is read-only after the daemon starts.

use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;
use std::path::PathBuf;

use super::paths::default_data_dir;

/// Default profile name applied during shutdown for thermal safety
pub const DEFAULT_SAFE_BOOT_PROFILE: &str = "100% PWM";

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
fn default_shutdown_profile() -> ProfileName {
    ProfileName::new(DEFAULT_SAFE_BOOT_PROFILE)
}

/// Profile name identifier for referencing saved profiles
///
/// Provides type safety for profile name strings, preventing accidental
/// use of arbitrary strings where profile names are expected.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileName(String);

impl ProfileName {
    /// Create a new profile name
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Get the underlying string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for ProfileName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for ProfileName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for ProfileName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProfileName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for ProfileName {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ProfileName {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
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

/// Shutdown configuration for safe boot profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownConfig {
    /// Enable safe boot profile on shutdown (default: true)
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Name of the profile to apply on shutdown (default: "100% PWM")
    #[serde(default = "default_shutdown_profile")]
    pub profile: ProfileName,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            profile: default_shutdown_profile(),
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

    /// Shutdown configuration for safe boot profile
    #[serde(default)]
    pub shutdown: ShutdownConfig,
}

impl Default for StaticConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            data_dir: default_data_dir(),
            reconnect: ReconnectConfig::default(),
            shutdown: ShutdownConfig::default(),
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

    #[test]
    fn test_shutdown_config_defaults() {
        let config = ShutdownConfig::default();
        assert!(config.enabled);
        assert_eq!(config.profile.as_str(), DEFAULT_SAFE_BOOT_PROFILE);
    }

    #[test]
    fn test_static_config_with_shutdown_section() {
        let toml_str = r#"
            data_dir = "/var/lib/openfan"

            [server]
            bind_address = "127.0.0.1"
            port = 3000
            communication_timeout = 1

            [shutdown]
            enabled = true
            profile = "Silent Mode"
        "#;

        let config = StaticConfig::from_toml(toml_str).unwrap();
        assert!(config.shutdown.enabled);
        assert_eq!(config.shutdown.profile.as_str(), "Silent Mode");
    }

    #[test]
    fn test_static_config_shutdown_defaults_when_missing() {
        // When shutdown section is missing, use defaults
        let toml_str = r#"
            data_dir = "/var/lib/openfan"
        "#;

        let config = StaticConfig::from_toml(toml_str).unwrap();
        assert!(config.shutdown.enabled);
        assert_eq!(config.shutdown.profile.as_str(), DEFAULT_SAFE_BOOT_PROFILE);
    }

    #[test]
    fn test_static_config_shutdown_partial() {
        // Partial shutdown section - missing fields use defaults
        let toml_str = r#"
            [shutdown]
            profile = "Custom Profile"
        "#;

        let config = StaticConfig::from_toml(toml_str).unwrap();
        assert!(config.shutdown.enabled); // default
        assert_eq!(config.shutdown.profile.as_str(), "Custom Profile");
    }

    #[test]
    fn test_static_config_shutdown_disabled() {
        let toml_str = r#"
            [shutdown]
            enabled = false
        "#;

        let config = StaticConfig::from_toml(toml_str).unwrap();
        assert!(!config.shutdown.enabled);
        assert_eq!(config.shutdown.profile.as_str(), DEFAULT_SAFE_BOOT_PROFILE);
    }

    // ProfileName tests - all test actual implementations we wrote
    #[test]
    fn test_profile_name_new() {
        let name = ProfileName::new("Test Profile");
        assert_eq!(name.as_str(), "Test Profile");
    }

    #[test]
    fn test_profile_name_from_string() {
        let name: ProfileName = String::from("From String").into();
        assert_eq!(name.as_str(), "From String");
    }

    #[test]
    fn test_profile_name_from_str() {
        let name: ProfileName = "From &str".into();
        assert_eq!(name.as_str(), "From &str");
    }

    #[test]
    fn test_profile_name_deref() {
        let name = ProfileName::new("Deref Test");
        // Deref allows using str methods directly on ProfileName
        assert!(name.starts_with("Deref"));
        assert!(name.ends_with("Test"));
        assert_eq!(name.len(), 10);
    }

    #[test]
    fn test_profile_name_display() {
        let name = ProfileName::new("Display Test");
        assert_eq!(format!("{}", name), "Display Test");
    }

    #[test]
    fn test_profile_name_clone_and_eq() {
        let name = ProfileName::new("Clone Test");
        let cloned = name.clone();
        assert_eq!(name, cloned);
        assert_ne!(name, ProfileName::new("Different"));
    }

    #[test]
    fn test_profile_name_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(ProfileName::new("Profile 1"));
        set.insert(ProfileName::new("Profile 2"));
        set.insert(ProfileName::new("Profile 1")); // duplicate should not increase size
        assert_eq!(set.len(), 2);
        assert!(set.contains(&ProfileName::new("Profile 1")));
    }

    #[test]
    fn test_profile_name_serde_roundtrip() {
        let name = ProfileName::new("Serde Test");
        // Serialize with serde_json (transparent means just the string)
        let serialized = serde_json::to_string(&name).unwrap();
        assert_eq!(serialized, "\"Serde Test\"");
        // Deserialize back
        let deserialized: ProfileName = serde_json::from_str(&serialized).unwrap();
        assert_eq!(name, deserialized);
    }

    #[test]
    fn test_profile_name_hashmap_lookup() {
        use std::collections::HashMap;
        let mut map: HashMap<String, i32> = HashMap::new();
        map.insert("Test Profile".to_string(), 42);

        let name = ProfileName::new("Test Profile");
        // Borrow<str> allows ProfileName to be used for HashMap<String, _> lookups
        assert_eq!(map.get(name.as_str()), Some(&42));
    }
}

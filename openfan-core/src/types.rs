//! Core types and data structures for OpenFAN

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Maximum number of fans supported
pub const MAX_FANS: usize = 10;

/// Fan control mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlMode {
    /// PWM (Pulse Width Modulation) mode - percentage-based
    Pwm,
    /// RPM (Revolutions Per Minute) mode - target speed
    Rpm,
}

/// Fan profile containing control mode and values for all fans
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanProfile {
    /// Control mode (pwm or rpm)
    #[serde(rename = "type")]
    pub control_mode: ControlMode,
    /// Values for each fan (10 values)
    pub values: Vec<u32>,
}

impl FanProfile {
    /// Create a new fan profile
    pub fn new(control_mode: ControlMode, values: Vec<u32>) -> Self {
        Self {
            control_mode,
            values,
        }
    }

    /// Validate that the profile has exactly 10 values
    pub fn validate(&self) -> Result<(), String> {
        if self.values.len() != MAX_FANS {
            return Err(format!(
                "Profile must have exactly {} values, got {}",
                MAX_FANS,
                self.values.len()
            ));
        }
        Ok(())
    }
}

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

/// Complete system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Server configuration
    pub server: ServerConfig,
    /// Hardware configuration
    pub hardware: HardwareConfig,
    /// Fan profiles (name -> profile)
    pub fan_profiles: HashMap<String, FanProfile>,
    /// Fan aliases (fan_id -> name)
    #[serde(
        serialize_with = "serialize_aliases",
        deserialize_with = "deserialize_aliases"
    )]
    pub fan_aliases: HashMap<u8, String>,
}

// Custom serialization for fan_aliases to use integers as keys in YAML
fn serialize_aliases<S>(aliases: &HashMap<u8, String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(Some(aliases.len()))?;
    for (k, v) in aliases {
        map.serialize_entry(k, v)?;
    }
    map.end()
}

// Custom deserialization for fan_aliases to handle both integer and string keys
fn deserialize_aliases<'de, D>(deserializer: D) -> Result<HashMap<u8, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct AliasesVisitor;

    impl<'de> Visitor<'de> for AliasesVisitor {
        type Value = HashMap<u8, String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a map with integer keys")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            let mut map = HashMap::with_capacity(access.size_hint().unwrap_or(0));

            while let Some((key, value)) = access.next_entry::<serde_json::Value, String>()? {
                let fan_id = if key.is_u64() {
                    key.as_u64().unwrap() as u8
                } else if key.is_string() {
                    key.as_str()
                        .unwrap()
                        .parse::<u8>()
                        .map_err(de::Error::custom)?
                } else {
                    return Err(de::Error::custom("key must be an integer or string"));
                };

                map.insert(fan_id, value);
            }

            Ok(map)
        }
    }

    deserializer.deserialize_map(AliasesVisitor)
}

impl Default for Config {
    fn default() -> Self {
        let mut fan_profiles = HashMap::new();
        fan_profiles.insert(
            "50% PWM".to_string(),
            FanProfile::new(ControlMode::Pwm, vec![50; MAX_FANS]),
        );
        fan_profiles.insert(
            "100% PWM".to_string(),
            FanProfile::new(ControlMode::Pwm, vec![100; MAX_FANS]),
        );
        fan_profiles.insert(
            "1000 RPM".to_string(),
            FanProfile::new(ControlMode::Rpm, vec![1000; MAX_FANS]),
        );

        let mut fan_aliases = HashMap::new();
        for i in 0..MAX_FANS as u8 {
            fan_aliases.insert(i, format!("Fan #{}", i + 1));
        }

        Self {
            server: ServerConfig::default(),
            hardware: HardwareConfig::default(),
            fan_profiles,
            fan_aliases,
        }
    }
}

/// API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    /// Status: "ok" or "fail"
    pub status: String,
    /// Optional message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Optional data payload
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<T> ApiResponse<T> {
    /// Create a successful response with data
    pub fn ok(data: T) -> Self {
        Self {
            status: "ok".to_string(),
            message: None,
            data: Some(data),
        }
    }

    /// Create a successful response with a message
    pub fn ok_with_message(message: String, data: T) -> Self {
        Self {
            status: "ok".to_string(),
            message: Some(message),
            data: Some(data),
        }
    }

    /// Create a failure response
    pub fn fail(message: String) -> Self {
        Self {
            status: "fail".to_string(),
            message: Some(message),
            data: None,
        }
    }
}

/// Fan status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanStatus {
    /// Fan ID (0-9)
    pub id: u8,
    /// Fan alias/name
    pub alias: String,
    /// Current RPM
    pub rpm: u32,
    /// Current PWM percentage
    pub pwm: u32,
}

/// Fan alias information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanAliasInfo {
    /// Fan ID
    pub fan_id: u8,
    /// Fan alias/name
    pub alias: String,
}

/// Map of fan ID to RPM values
pub type FanRpmMap = HashMap<u8, u32>;

/// Map of fan ID to PWM values
pub type FanPwmMap = HashMap<u8, u32>;

/// Hardware information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    /// Hardware version
    pub version: String,
    /// Additional hardware details
    #[serde(flatten)]
    pub details: HashMap<String, serde_json::Value>,
}

/// Firmware information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareInfo {
    /// Firmware version
    pub version: String,
    /// Additional firmware details
    #[serde(flatten)]
    pub details: HashMap<String, serde_json::Value>,
}

/// System information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    /// Hardware information
    pub hardware: HardwareInfo,
    /// Firmware information
    pub firmware: FirmwareInfo,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fan_profile_validation() {
        let profile = FanProfile::new(ControlMode::Pwm, vec![50; MAX_FANS]);
        assert!(profile.validate().is_ok());

        let invalid_profile = FanProfile::new(ControlMode::Pwm, vec![50; 5]);
        assert!(invalid_profile.validate().is_err());
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.fan_profiles.len(), 3);
        assert_eq!(config.fan_aliases.len(), MAX_FANS);
        assert_eq!(config.server.port, 3000);
    }

    #[test]
    fn test_api_response() {
        let response: ApiResponse<u32> = ApiResponse::ok(42);
        assert_eq!(response.status, "ok");
        assert_eq!(response.data, Some(42));

        let fail_response: ApiResponse<()> = ApiResponse::fail("error".to_string());
        assert_eq!(fail_response.status, "fail");
        assert!(fail_response.data.is_none());
    }

    #[test]
    fn test_control_mode_serialization() {
        let json = serde_json::to_string(&ControlMode::Pwm).unwrap();
        assert_eq!(json, r#""pwm""#);

        let json = serde_json::to_string(&ControlMode::Rpm).unwrap();
        assert_eq!(json, r#""rpm""#);
    }
}

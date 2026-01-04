//! API models for OpenFAN REST API
//!
//! This module contains request and response models for the OpenFAN REST API.

use crate::types::{ControlMode, FanProfile, SystemInfo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Generic API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ApiResponse<T> {
    #[serde(rename = "success")]
    Success { data: T },
    #[serde(rename = "error")]
    Error { error: String },
}

impl<T> ApiResponse<T> {
    /// Create a successful response
    pub fn success(data: T) -> Self {
        Self::Success { data }
    }

    /// Create an error response
    pub fn error(error: String) -> Self {
        Self::Error { error }
    }
}

/// Server information response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoResponse {
    /// Server version
    pub version: String,
    /// Detected board information
    pub board_info: crate::BoardInfo,
    /// Whether hardware is connected
    pub hardware_connected: bool,
    /// Connection status: "connected", "disconnected", "reconnecting", or "mock"
    #[serde(default)]
    pub connection_status: String,
    /// Number of successful reconnections since server start
    #[serde(default)]
    pub reconnect_count: u32,
    /// Whether automatic reconnection is enabled
    #[serde(default)]
    pub reconnection_enabled: bool,
    /// Seconds since last disconnection (None if never disconnected or currently connected)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_since_disconnect_secs: Option<u64>,
    /// Server uptime in seconds
    pub uptime: u64,
    /// Software information
    pub software: String,
    /// Hardware information (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hardware: Option<String>,
    /// Firmware information (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware: Option<String>,
}

/// Fan status response containing all fan RPMs and PWMs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanStatusResponse {
    /// Map of fan ID to current RPM
    #[serde(
        serialize_with = "serialize_u8_map",
        deserialize_with = "deserialize_u8_map"
    )]
    pub rpms: HashMap<u8, u32>,
    /// Map of fan ID to current PWM percentage
    #[serde(
        serialize_with = "serialize_u8_map",
        deserialize_with = "deserialize_u8_map"
    )]
    pub pwms: HashMap<u8, u32>,
}

/// Single fan RPM response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanRpmResponse {
    /// Fan ID
    pub fan_id: u8,
    /// Current RPM
    pub rpm: u32,
}

/// Fan control request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanControlRequest {
    /// Control mode (pwm or rpm)
    pub mode: ControlMode,
    /// Control value (percentage for PWM, RPM for RPM mode)
    pub value: u32,
}

/// Profile response containing all profiles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileResponse {
    /// Map of profile name to profile data
    pub profiles: HashMap<String, FanProfile>,
}

/// Profile application request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileRequest {
    /// Profile name to apply
    pub name: String,
}

/// Profile addition request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddProfileRequest {
    /// Profile name
    pub name: String,
    /// Profile data
    pub profile: FanProfile,
}

/// Alias response containing all fan aliases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasResponse {
    /// Map of fan ID to alias
    #[serde(
        serialize_with = "serialize_u8_string_map",
        deserialize_with = "deserialize_u8_string_map"
    )]
    pub aliases: HashMap<u8, String>,
}

/// Alias setting request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasRequest {
    /// New alias for the fan
    pub alias: String,
}

/// Zone response containing all zones
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneResponse {
    /// Map of zone name to zone data
    pub zones: HashMap<String, crate::Zone>,
}

/// Single zone response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleZoneResponse {
    /// Zone data
    pub zone: crate::Zone,
}

/// Zone addition request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddZoneRequest {
    /// Zone name
    pub name: String,
    /// Fans to include in the zone (controller + fan_id pairs)
    pub fans: Vec<crate::ZoneFan>,
    /// Optional description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Zone update request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateZoneRequest {
    /// Fans to include in the zone (controller + fan_id pairs)
    pub fans: Vec<crate::ZoneFan>,
    /// Optional description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Thermal curve response containing all curves
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalCurveResponse {
    /// Map of curve name to curve data
    pub curves: std::collections::HashMap<String, crate::ThermalCurve>,
}

/// Single thermal curve response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleCurveResponse {
    /// Curve data
    pub curve: crate::ThermalCurve,
}

/// Thermal curve addition request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddCurveRequest {
    /// Curve name
    pub name: String,
    /// Curve points
    pub points: Vec<crate::CurvePoint>,
    /// Optional description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Thermal curve update request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCurveRequest {
    /// Curve points
    pub points: Vec<crate::CurvePoint>,
    /// Optional description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Interpolation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterpolateResponse {
    /// Temperature that was queried
    pub temperature: f32,
    /// Interpolated PWM value
    pub pwm: u8,
}

/// CFM mappings list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfmListResponse {
    /// Map of port ID to CFM@100%
    #[serde(
        serialize_with = "serialize_u8_f32_map",
        deserialize_with = "deserialize_u8_f32_map"
    )]
    pub mappings: HashMap<u8, f32>,
}

/// Single CFM mapping response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfmGetResponse {
    /// Port ID
    pub port: u8,
    /// CFM value at 100% PWM
    pub cfm_at_100: f32,
}

/// CFM mapping set request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetCfmRequest {
    /// CFM value at 100% PWM
    pub cfm_at_100: f32,
}

/// System information response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfoResponse {
    /// System information
    pub system_info: SystemInfo,
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Service status
    pub status: String,
    /// Timestamp
    pub timestamp: u64,
}

// Custom serialization for HashMap<u8, u32>
fn serialize_u8_map<S>(map: &HashMap<u8, u32>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;
    let mut ser_map = serializer.serialize_map(Some(map.len()))?;
    for (k, v) in map {
        ser_map.serialize_entry(&k.to_string(), v)?;
    }
    ser_map.end()
}

// Custom deserialization for HashMap<u8, u32>
fn deserialize_u8_map<'de, D>(deserializer: D) -> Result<HashMap<u8, u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct U8MapVisitor;

    impl<'de> Visitor<'de> for U8MapVisitor {
        type Value = HashMap<u8, u32>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a map with string keys representing u8 values")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            let mut map = HashMap::with_capacity(access.size_hint().unwrap_or(0));

            while let Some((key, value)) = access.next_entry::<String, u32>()? {
                let fan_id = key
                    .parse::<u8>()
                    .map_err(|_| de::Error::custom(format!("Invalid fan ID: {}", key)))?;
                map.insert(fan_id, value);
            }

            Ok(map)
        }
    }

    deserializer.deserialize_map(U8MapVisitor)
}

// Custom serialization for HashMap<u8, String>
fn serialize_u8_string_map<S>(map: &HashMap<u8, String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;
    let mut ser_map = serializer.serialize_map(Some(map.len()))?;
    for (k, v) in map {
        ser_map.serialize_entry(&k.to_string(), v)?;
    }
    ser_map.end()
}

// Custom deserialization for HashMap<u8, String>
fn deserialize_u8_string_map<'de, D>(deserializer: D) -> Result<HashMap<u8, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct U8StringMapVisitor;

    impl<'de> Visitor<'de> for U8StringMapVisitor {
        type Value = HashMap<u8, String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a map with string keys representing u8 values")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            let mut map = HashMap::with_capacity(access.size_hint().unwrap_or(0));

            while let Some((key, value)) = access.next_entry::<String, String>()? {
                let fan_id = key
                    .parse::<u8>()
                    .map_err(|_| de::Error::custom(format!("Invalid fan ID: {}", key)))?;
                map.insert(fan_id, value);
            }

            Ok(map)
        }
    }

    deserializer.deserialize_map(U8StringMapVisitor)
}

// Custom serialization for HashMap<u8, f32>
fn serialize_u8_f32_map<S>(map: &HashMap<u8, f32>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;
    let mut ser_map = serializer.serialize_map(Some(map.len()))?;
    for (k, v) in map {
        ser_map.serialize_entry(&k.to_string(), v)?;
    }
    ser_map.end()
}

// Custom deserialization for HashMap<u8, f32>
fn deserialize_u8_f32_map<'de, D>(deserializer: D) -> Result<HashMap<u8, f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct U8F32MapVisitor;

    impl<'de> Visitor<'de> for U8F32MapVisitor {
        type Value = HashMap<u8, f32>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a map with string keys representing u8 values and f32 values")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            let mut map = HashMap::with_capacity(access.size_hint().unwrap_or(0));

            while let Some((key, value)) = access.next_entry::<String, f32>()? {
                let port_id = key
                    .parse::<u8>()
                    .map_err(|_| de::Error::custom(format!("Invalid port ID: {}", key)))?;
                map.insert(port_id, value);
            }

            Ok(map)
        }
    }

    deserializer.deserialize_map(U8F32MapVisitor)
}

// ============================================================================
// Controller Management Types (Multi-Controller Support)
// ============================================================================

/// Controller info returned by the API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerInfo {
    /// Unique identifier for this controller
    pub id: String,
    /// Board name
    pub board_name: String,
    /// Number of fan ports
    pub fan_count: usize,
    /// Optional description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether this controller is in mock mode
    pub mock_mode: bool,
    /// Connection status
    pub connected: bool,
}

/// Response for listing all controllers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllersListResponse {
    /// Total number of controllers
    pub count: usize,
    /// List of controller info
    pub controllers: Vec<ControllerInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ControlMode;

    #[test]
    fn test_api_response_success() {
        let response = ApiResponse::success("test data");
        match response {
            ApiResponse::Success { data } => assert_eq!(data, "test data"),
            _ => panic!("Expected success response"),
        }
    }

    #[test]
    fn test_api_response_error() {
        let response: ApiResponse<()> = ApiResponse::error("test error".to_string());
        match response {
            ApiResponse::Error { error } => assert_eq!(error, "test error"),
            _ => panic!("Expected error response"),
        }
    }

    #[test]
    fn test_fan_control_request_serialization() {
        let request = FanControlRequest {
            mode: ControlMode::Pwm,
            value: 75,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("pwm"));
        assert!(json.contains("75"));
    }

    #[test]
    fn test_profile_request_serialization() {
        let request = ProfileRequest {
            name: "Test Profile".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("Test Profile"));
    }

    #[test]
    fn test_alias_request_serialization() {
        let request = AliasRequest {
            alias: "CPU Fan".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("CPU Fan"));
    }

    #[test]
    fn test_info_response() {
        let board_info = crate::BoardType::OpenFanStandard.to_board_info();
        let response = InfoResponse {
            version: env!("CARGO_PKG_VERSION").to_string(),
            board_info,
            hardware_connected: true,
            connection_status: "connected".to_string(),
            reconnect_count: 0,
            reconnection_enabled: true,
            time_since_disconnect_secs: None,
            uptime: 3600,
            software: format!("OpenFAN Server v{}", env!("CARGO_PKG_VERSION")),
            hardware: Some("Hardware v1.0".to_string()),
            firmware: Some("Firmware v1.0".to_string()),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains(env!("CARGO_PKG_VERSION")));
        assert!(json.contains("true"));
        assert!(json.contains("3600"));
        assert!(json.contains("connected"));
        assert!(json.contains("reconnection_enabled"));
    }

    #[test]
    fn test_fan_status_response() {
        let mut rpms = HashMap::new();
        let mut pwms = HashMap::new();
        rpms.insert(0, 1200);
        rpms.insert(1, 1500);
        pwms.insert(0, 50);
        pwms.insert(1, 75);

        let response = FanStatusResponse { rpms, pwms };
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("1200"));
        assert!(json.contains("1500"));
        assert!(json.contains("50"));
        assert!(json.contains("75"));
    }
}

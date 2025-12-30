//! CFM mapping data - mutable via API
//!
//! Stored in `{data_dir}/cfm_mappings.toml`
//!
//! Provides optional per-port PWM→CFM conversion for display purposes.
//! CFM (Cubic Feet per Minute) values are calculated using linear interpolation:
//! `cfm = (pwm / 100.0) * cfm_at_100`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Maximum allowed CFM value (reasonable upper limit for PC fans)
pub const MAX_CFM: f32 = 500.0;

/// CFM mapping data stored in cfm_mappings.toml
///
/// Maps port IDs to their CFM@100% values for display purposes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CfmMappingData {
    /// Port ID to CFM@100% mapping
    ///
    /// Keys are stringified port IDs ("0", "1", etc.) for TOML compatibility.
    #[serde(
        serialize_with = "serialize_mappings",
        deserialize_with = "deserialize_mappings",
        default
    )]
    pub mappings: HashMap<u8, f32>,
}

impl CfmMappingData {
    /// Create empty CFM mapping data.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get CFM@100% for a port.
    pub fn get(&self, port: u8) -> Option<f32> {
        self.mappings.get(&port).copied()
    }

    /// Set CFM@100% for a port.
    pub fn set(&mut self, port: u8, cfm_at_100: f32) {
        self.mappings.insert(port, cfm_at_100);
    }

    /// Remove CFM mapping for a port.
    ///
    /// Returns `true` if a mapping was removed, `false` if none existed.
    pub fn remove(&mut self, port: u8) -> bool {
        self.mappings.remove(&port).is_some()
    }

    /// Check if a port has a CFM mapping.
    pub fn contains(&self, port: u8) -> bool {
        self.mappings.contains_key(&port)
    }

    /// Check if any mappings exist.
    pub fn is_empty(&self) -> bool {
        self.mappings.is_empty()
    }

    /// Get the number of mappings.
    pub fn len(&self) -> usize {
        self.mappings.len()
    }

    /// Calculate CFM from PWM for a port.
    ///
    /// Returns `None` if no mapping exists for the port.
    /// Uses linear interpolation: `cfm = (pwm / 100.0) * cfm_at_100`
    pub fn calculate_cfm(&self, port: u8, pwm: u32) -> Option<f32> {
        self.mappings
            .get(&port)
            .map(|cfm_at_100| (pwm as f32 / 100.0) * cfm_at_100)
    }

    /// Parse CfmMappingData from TOML string.
    pub fn from_toml(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }

    /// Serialize CfmMappingData to TOML string.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Validate a CFM value.
    ///
    /// Returns an error message if the value is invalid.
    pub fn validate_cfm(cfm: f32) -> Result<(), String> {
        if cfm <= 0.0 {
            return Err("CFM value must be positive".to_string());
        }
        if cfm > MAX_CFM {
            return Err(format!("CFM value must be <= {}", MAX_CFM));
        }
        Ok(())
    }
}

// Custom serialization: HashMap<u8, f32> -> HashMap<String, f32> for TOML
fn serialize_mappings<S>(mappings: &HashMap<u8, f32>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(Some(mappings.len()))?;
    for (k, v) in mappings {
        map.serialize_entry(&k.to_string(), v)?;
    }
    map.end()
}

// Custom deserialization: HashMap<String, f32> -> HashMap<u8, f32>
fn deserialize_mappings<'de, D>(deserializer: D) -> Result<HashMap<u8, f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let string_map: HashMap<String, f32> = HashMap::deserialize(deserializer)?;

    string_map
        .into_iter()
        .map(|(k, v)| {
            k.parse::<u8>()
                .map(|id| (id, v))
                .map_err(|_| D::Error::custom(format!("invalid port ID: {}", k)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_empty() {
        let data = CfmMappingData::default();
        assert!(data.is_empty());
        assert_eq!(data.len(), 0);
    }

    #[test]
    fn test_set_and_get() {
        let mut data = CfmMappingData::new();
        data.set(0, 45.0);
        data.set(1, 60.0);

        assert_eq!(data.get(0), Some(45.0));
        assert_eq!(data.get(1), Some(60.0));
        assert_eq!(data.get(2), None);
        assert!(data.contains(0));
        assert!(!data.contains(2));
    }

    #[test]
    fn test_remove() {
        let mut data = CfmMappingData::new();
        data.set(0, 45.0);

        assert!(data.remove(0));
        assert!(!data.remove(0)); // Already removed
        assert_eq!(data.get(0), None);
    }

    #[test]
    fn test_calculate_cfm() {
        let mut data = CfmMappingData::new();
        data.set(0, 45.0);

        // 100% PWM = full CFM
        assert_eq!(data.calculate_cfm(0, 100), Some(45.0));

        // 50% PWM = half CFM
        assert_eq!(data.calculate_cfm(0, 50), Some(22.5));

        // 0% PWM = 0 CFM
        assert_eq!(data.calculate_cfm(0, 0), Some(0.0));

        // 75% PWM
        assert_eq!(data.calculate_cfm(0, 75), Some(33.75));

        // No mapping for port
        assert_eq!(data.calculate_cfm(1, 50), None);
    }

    #[test]
    fn test_serialization() {
        let mut data = CfmMappingData::new();
        data.set(0, 45.0);
        data.set(3, 60.5);

        let toml_str = data.to_toml().unwrap();
        assert!(toml_str.contains("[mappings]"));
        assert!(toml_str.contains("45"));
        assert!(toml_str.contains("60.5"));
    }

    #[test]
    fn test_deserialization() {
        let toml_str = r#"
            [mappings]
            0 = 45.0
            1 = 60.0
            5 = 30.5
        "#;

        let data = CfmMappingData::from_toml(toml_str).unwrap();
        assert_eq!(data.get(0), Some(45.0));
        assert_eq!(data.get(1), Some(60.0));
        assert_eq!(data.get(5), Some(30.5));
        assert_eq!(data.get(2), None);
    }

    #[test]
    fn test_roundtrip() {
        let mut original = CfmMappingData::new();
        original.set(0, 45.0);
        original.set(1, 60.0);
        original.set(9, 30.5);

        let toml_str = original.to_toml().unwrap();
        let restored = CfmMappingData::from_toml(&toml_str).unwrap();

        assert_eq!(original.mappings.len(), restored.mappings.len());
        for (port, cfm) in &original.mappings {
            assert_eq!(restored.get(*port), Some(*cfm));
        }
    }

    #[test]
    fn test_validate_cfm() {
        // Valid values
        assert!(CfmMappingData::validate_cfm(1.0).is_ok());
        assert!(CfmMappingData::validate_cfm(45.0).is_ok());
        assert!(CfmMappingData::validate_cfm(MAX_CFM).is_ok());

        // Invalid values
        assert!(CfmMappingData::validate_cfm(0.0).is_err());
        assert!(CfmMappingData::validate_cfm(-1.0).is_err());
        assert!(CfmMappingData::validate_cfm(MAX_CFM + 1.0).is_err());
    }

    #[test]
    fn test_empty_toml() {
        let toml_str = "";
        let data = CfmMappingData::from_toml(toml_str).unwrap();
        assert!(data.is_empty());
    }

    #[test]
    fn test_empty_mappings_toml() {
        let toml_str = "[mappings]";
        let data = CfmMappingData::from_toml(toml_str).unwrap();
        assert!(data.is_empty());
    }
}

//! Fan profile data - mutable via API
//!
//! Stored in `{data_dir}/profiles.toml`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::hardware::MAX_FANS;
use crate::types::{ControlMode, FanProfile};

/// Fan profile data stored in profiles.toml
///
/// Maps profile names to their definitions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileData {
    /// Profile name to profile definition mapping
    #[serde(default)]
    pub profiles: HashMap<String, FanProfile>,
}

impl ProfileData {
    /// Create profile data with default profiles.
    pub fn with_defaults() -> Self {
        let mut profiles = HashMap::new();

        profiles.insert(
            "50% PWM".to_string(),
            FanProfile::new(ControlMode::Pwm, vec![50; MAX_FANS]),
        );
        profiles.insert(
            "100% PWM".to_string(),
            FanProfile::new(ControlMode::Pwm, vec![100; MAX_FANS]),
        );
        profiles.insert(
            "1000 RPM".to_string(),
            FanProfile::new(ControlMode::Rpm, vec![1000; MAX_FANS]),
        );

        Self { profiles }
    }

    /// Get a profile by name.
    pub fn get(&self, name: &str) -> Option<&FanProfile> {
        self.profiles.get(name)
    }

    /// Insert a profile.
    pub fn insert(&mut self, name: String, profile: FanProfile) {
        self.profiles.insert(name, profile);
    }

    /// Remove a profile by name.
    pub fn remove(&mut self, name: &str) -> Option<FanProfile> {
        self.profiles.remove(name)
    }

    /// Check if a profile exists.
    pub fn contains(&self, name: &str) -> bool {
        self.profiles.contains_key(name)
    }

    /// Get all profile names.
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.profiles.keys()
    }

    /// Parse ProfileData from TOML string.
    pub fn from_toml(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }

    /// Serialize ProfileData to TOML string.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_profiles_empty() {
        let data = ProfileData::default();
        assert!(data.profiles.is_empty());
    }

    #[test]
    fn test_with_defaults() {
        let data = ProfileData::with_defaults();
        assert_eq!(data.profiles.len(), 3);
        assert!(data.contains("50% PWM"));
        assert!(data.contains("100% PWM"));
        assert!(data.contains("1000 RPM"));
    }

    #[test]
    fn test_profile_operations() {
        let mut data = ProfileData::default();

        let profile = FanProfile::new(ControlMode::Pwm, vec![75; MAX_FANS]);
        data.insert("Custom".to_string(), profile);

        assert!(data.contains("Custom"));
        assert_eq!(data.get("Custom").unwrap().values[0], 75);

        let removed = data.remove("Custom");
        assert!(removed.is_some());
        assert!(!data.contains("Custom"));
    }

    #[test]
    fn test_profile_serialization() {
        let data = ProfileData::with_defaults();
        let toml_str = data.to_toml().unwrap();

        assert!(toml_str.contains("[profiles.\"50% PWM\"]"));
        assert!(toml_str.contains("[profiles.\"100% PWM\"]"));
        assert!(toml_str.contains("[profiles.\"1000 RPM\"]"));
        assert!(toml_str.contains("type = \"pwm\""));
    }

    #[test]
    fn test_profile_deserialization() {
        let toml_str = r#"
            [profiles."Silent Mode"]
            type = "pwm"
            values = [30, 30, 30, 30, 30, 30, 30, 30, 30, 30]

            [profiles."Performance"]
            type = "rpm"
            values = [2000, 2000, 2000, 2000, 2000, 2000, 2000, 2000, 2000, 2000]
        "#;

        let data = ProfileData::from_toml(toml_str).unwrap();
        assert_eq!(data.profiles.len(), 2);

        let silent = data.get("Silent Mode").unwrap();
        assert_eq!(silent.control_mode, ControlMode::Pwm);
        assert_eq!(silent.values[0], 30);

        let perf = data.get("Performance").unwrap();
        assert_eq!(perf.control_mode, ControlMode::Rpm);
        assert_eq!(perf.values[0], 2000);
    }

    #[test]
    fn test_profile_roundtrip() {
        let original = ProfileData::with_defaults();
        let toml_str = original.to_toml().unwrap();
        let restored = ProfileData::from_toml(&toml_str).unwrap();

        assert_eq!(original.profiles.len(), restored.profiles.len());
        for name in original.names() {
            let orig_profile = original.get(name).unwrap();
            let restored_profile = restored.get(name).unwrap();
            assert_eq!(orig_profile.control_mode, restored_profile.control_mode);
            assert_eq!(orig_profile.values, restored_profile.values);
        }
    }
}

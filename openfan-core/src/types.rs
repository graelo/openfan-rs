//! Core types and data structures for OpenFAN

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Import MAX_FANS from board definitions
use crate::board::MAX_FANS;

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
    /// Creates a new fan profile
    pub fn new(control_mode: ControlMode, values: Vec<u32>) -> Self {
        Self {
            control_mode,
            values,
        }
    }

    /// Validates that the profile has the correct number of values for the board
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

/// Map of fan ID to RPM values
pub type FanRpmMap = HashMap<u8, u32>;

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
    fn test_control_mode_serialization() {
        let json = serde_json::to_string(&ControlMode::Pwm).unwrap();
        assert_eq!(json, r#""pwm""#);

        let json = serde_json::to_string(&ControlMode::Rpm).unwrap();
        assert_eq!(json, r#""rpm""#);
    }

    #[test]
    fn test_fan_profile_empty_values() {
        let profile = FanProfile::new(ControlMode::Pwm, vec![]);
        assert!(
            profile.validate().is_err(),
            "Profile with empty values should fail validation"
        );
    }

    #[test]
    fn test_fan_profile_too_many_values() {
        let profile = FanProfile::new(ControlMode::Pwm, vec![50; MAX_FANS + 5]);
        assert!(
            profile.validate().is_err(),
            "Profile with too many values should fail validation"
        );
    }

    #[test]
    fn test_fan_profile_one_less_value() {
        let profile = FanProfile::new(ControlMode::Pwm, vec![50; MAX_FANS - 1]);
        assert!(
            profile.validate().is_err(),
            "Profile with one less value should fail validation"
        );
    }
}

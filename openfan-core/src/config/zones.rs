//! Zone data - mutable via API
//!
//! Stored in `{data_dir}/zones.toml`
//!
//! Zones group multiple fan ports for coordinated control across controllers.
//! Each port can belong to at most one zone (exclusive membership).
//! Zones are global and can span multiple controllers.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A fan reference within a zone, identifying both controller and fan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneFan {
    /// Controller ID this fan belongs to
    pub controller: String,
    /// Fan port ID on the controller (0-based)
    pub fan_id: u8,
}

impl ZoneFan {
    /// Create a new zone fan reference.
    pub fn new(controller: impl Into<String>, fan_id: u8) -> Self {
        Self {
            controller: controller.into(),
            fan_id,
        }
    }
}

/// A zone grouping multiple fan ports for coordinated control.
///
/// Zones can span multiple controllers, allowing fans from different
/// controllers to be controlled together (e.g., all intake fans).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Zone {
    /// Human-readable zone name
    pub name: String,
    /// Fans belonging to this zone (controller + fan_id pairs)
    pub fans: Vec<ZoneFan>,
    /// Optional description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Zone {
    /// Create a new zone with the given name and fans.
    pub fn new(name: impl Into<String>, fans: Vec<ZoneFan>) -> Self {
        Self {
            name: name.into(),
            fans,
            description: None,
        }
    }

    /// Create a new zone with description.
    pub fn with_description(
        name: impl Into<String>,
        fans: Vec<ZoneFan>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            fans,
            description: Some(description.into()),
        }
    }

    /// Get all fans for a specific controller.
    pub fn fans_for_controller(&self, controller_id: &str) -> Vec<u8> {
        self.fans
            .iter()
            .filter(|f| f.controller == controller_id)
            .map(|f| f.fan_id)
            .collect()
    }

    /// Check if this zone contains a specific fan.
    pub fn contains_fan(&self, controller_id: &str, fan_id: u8) -> bool {
        self.fans
            .iter()
            .any(|f| f.controller == controller_id && f.fan_id == fan_id)
    }
}

/// Zone data stored in zones.toml
///
/// Maps zone names to their definitions.
/// Zones are global and can span multiple controllers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ZoneData {
    /// Zone name to zone definition mapping
    #[serde(default)]
    pub zones: HashMap<String, Zone>,
}

impl ZoneData {
    /// Get a zone by name.
    pub fn get(&self, name: &str) -> Option<&Zone> {
        self.zones.get(name)
    }

    /// Insert a zone.
    pub fn insert(&mut self, name: String, zone: Zone) {
        self.zones.insert(name, zone);
    }

    /// Remove a zone by name.
    pub fn remove(&mut self, name: &str) -> Option<Zone> {
        self.zones.remove(name)
    }

    /// Check if a zone exists.
    pub fn contains(&self, name: &str) -> bool {
        self.zones.contains_key(name)
    }

    /// Get all zone names.
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.zones.keys()
    }

    /// Find which zone contains a given fan on a specific controller.
    ///
    /// Returns the zone name if the fan is in a zone, None otherwise.
    pub fn find_zone_for_fan(&self, controller_id: &str, fan_id: u8) -> Option<&str> {
        for (name, zone) in &self.zones {
            if zone.contains_fan(controller_id, fan_id) {
                return Some(name);
            }
        }
        None
    }

    /// Check if a fan is already assigned to any zone.
    pub fn is_fan_assigned(&self, controller_id: &str, fan_id: u8) -> bool {
        self.find_zone_for_fan(controller_id, fan_id).is_some()
    }

    /// Get all zones that include fans from a specific controller.
    pub fn zones_for_controller(&self, controller_id: &str) -> Vec<&str> {
        self.zones
            .iter()
            .filter(|(_, zone)| zone.fans.iter().any(|f| f.controller == controller_id))
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// Parse ZoneData from TOML string.
    pub fn from_toml(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }

    /// Serialize ZoneData to TOML string.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_zones_empty() {
        let data = ZoneData::default();
        assert!(data.zones.is_empty());
    }

    #[test]
    fn test_zone_fan_creation() {
        let fan = ZoneFan::new("main", 0);
        assert_eq!(fan.controller, "main");
        assert_eq!(fan.fan_id, 0);
    }

    #[test]
    fn test_zone_creation() {
        let fans = vec![
            ZoneFan::new("main", 0),
            ZoneFan::new("main", 1),
            ZoneFan::new("main", 2),
        ];
        let zone = Zone::new("intake", fans);
        assert_eq!(zone.name, "intake");
        assert_eq!(zone.fans.len(), 3);
        assert!(zone.description.is_none());

        let fans = vec![ZoneFan::new("main", 3), ZoneFan::new("main", 4)];
        let zone_with_desc = Zone::with_description("exhaust", fans, "Rear exhaust fans");
        assert_eq!(zone_with_desc.name, "exhaust");
        assert_eq!(
            zone_with_desc.description,
            Some("Rear exhaust fans".to_string())
        );
    }

    #[test]
    fn test_zone_operations() {
        let mut data = ZoneData::default();

        let fans = vec![
            ZoneFan::new("main", 0),
            ZoneFan::new("main", 1),
            ZoneFan::new("main", 2),
        ];
        let zone = Zone::new("intake", fans);
        data.insert("intake".to_string(), zone);

        assert!(data.contains("intake"));
        assert_eq!(data.get("intake").unwrap().fans.len(), 3);

        let removed = data.remove("intake");
        assert!(removed.is_some());
        assert!(!data.contains("intake"));
    }

    #[test]
    fn test_find_zone_for_fan() {
        let mut data = ZoneData::default();

        let intake_fans = vec![
            ZoneFan::new("main", 0),
            ZoneFan::new("main", 1),
            ZoneFan::new("gpu", 0),
        ];
        data.insert("intake".to_string(), Zone::new("intake", intake_fans));

        let exhaust_fans = vec![ZoneFan::new("main", 3), ZoneFan::new("main", 4)];
        data.insert("exhaust".to_string(), Zone::new("exhaust", exhaust_fans));

        assert_eq!(data.find_zone_for_fan("main", 0), Some("intake"));
        assert_eq!(data.find_zone_for_fan("main", 1), Some("intake"));
        assert_eq!(data.find_zone_for_fan("gpu", 0), Some("intake"));
        assert_eq!(data.find_zone_for_fan("main", 3), Some("exhaust"));
        assert_eq!(data.find_zone_for_fan("main", 5), None);
        assert_eq!(data.find_zone_for_fan("gpu", 1), None);

        assert!(data.is_fan_assigned("main", 0));
        assert!(!data.is_fan_assigned("main", 5));
    }

    #[test]
    fn test_zone_fans_for_controller() {
        let fans = vec![
            ZoneFan::new("main", 0),
            ZoneFan::new("main", 1),
            ZoneFan::new("gpu", 0),
            ZoneFan::new("gpu", 1),
        ];
        let zone = Zone::new("cooling", fans);

        let main_fans = zone.fans_for_controller("main");
        assert_eq!(main_fans, vec![0, 1]);

        let gpu_fans = zone.fans_for_controller("gpu");
        assert_eq!(gpu_fans, vec![0, 1]);

        let other_fans = zone.fans_for_controller("other");
        assert!(other_fans.is_empty());
    }

    #[test]
    fn test_zones_for_controller() {
        let mut data = ZoneData::default();

        let intake_fans = vec![ZoneFan::new("main", 0), ZoneFan::new("gpu", 0)];
        data.insert("intake".to_string(), Zone::new("intake", intake_fans));

        let exhaust_fans = vec![ZoneFan::new("main", 3)];
        data.insert("exhaust".to_string(), Zone::new("exhaust", exhaust_fans));

        let main_zones = data.zones_for_controller("main");
        assert_eq!(main_zones.len(), 2);
        assert!(main_zones.contains(&"intake"));
        assert!(main_zones.contains(&"exhaust"));

        let gpu_zones = data.zones_for_controller("gpu");
        assert_eq!(gpu_zones.len(), 1);
        assert!(gpu_zones.contains(&"intake"));

        let other_zones = data.zones_for_controller("other");
        assert!(other_zones.is_empty());
    }

    #[test]
    fn test_zone_serialization() {
        let mut data = ZoneData::default();
        let intake_fans = vec![
            ZoneFan::new("main", 0),
            ZoneFan::new("main", 1),
            ZoneFan::new("main", 2),
        ];
        data.insert(
            "intake".to_string(),
            Zone::with_description("intake", intake_fans, "Front intake fans"),
        );

        let exhaust_fans = vec![ZoneFan::new("main", 3), ZoneFan::new("main", 4)];
        data.insert("exhaust".to_string(), Zone::new("exhaust", exhaust_fans));

        let toml_str = data.to_toml().unwrap();

        assert!(toml_str.contains("[zones.intake]"));
        assert!(toml_str.contains("[zones.exhaust]"));
        assert!(toml_str.contains("description = \"Front intake fans\""));
        // Should contain fan entries with controller and fan_id
        assert!(toml_str.contains("controller"));
        assert!(toml_str.contains("fan_id"));
    }

    #[test]
    fn test_zone_deserialization() {
        let toml_str = r#"
            [zones.intake]
            name = "intake"
            description = "Front intake fans"
            [[zones.intake.fans]]
            controller = "main"
            fan_id = 0
            [[zones.intake.fans]]
            controller = "main"
            fan_id = 1
            [[zones.intake.fans]]
            controller = "main"
            fan_id = 2

            [zones.exhaust]
            name = "exhaust"
            [[zones.exhaust.fans]]
            controller = "main"
            fan_id = 3
            [[zones.exhaust.fans]]
            controller = "main"
            fan_id = 4
        "#;

        let data = ZoneData::from_toml(toml_str).unwrap();
        assert_eq!(data.zones.len(), 2);

        let intake = data.get("intake").unwrap();
        assert_eq!(intake.name, "intake");
        assert_eq!(intake.fans.len(), 3);
        assert_eq!(intake.fans[0].controller, "main");
        assert_eq!(intake.fans[0].fan_id, 0);
        assert_eq!(intake.description, Some("Front intake fans".to_string()));

        let exhaust = data.get("exhaust").unwrap();
        assert_eq!(exhaust.name, "exhaust");
        assert_eq!(exhaust.fans.len(), 2);
        assert!(exhaust.description.is_none());
    }

    #[test]
    fn test_zone_roundtrip() {
        let mut original = ZoneData::default();
        let intake_fans = vec![
            ZoneFan::new("main", 0),
            ZoneFan::new("main", 1),
            ZoneFan::new("gpu", 0),
        ];
        original.insert(
            "intake".to_string(),
            Zone::with_description("intake", intake_fans, "Front fans"),
        );

        let exhaust_fans = vec![ZoneFan::new("main", 3), ZoneFan::new("main", 4)];
        original.insert("exhaust".to_string(), Zone::new("exhaust", exhaust_fans));

        let toml_str = original.to_toml().unwrap();
        let restored = ZoneData::from_toml(&toml_str).unwrap();

        assert_eq!(original.zones.len(), restored.zones.len());
        for name in original.names() {
            let orig_zone = original.get(name).unwrap();
            let restored_zone = restored.get(name).unwrap();
            assert_eq!(orig_zone.name, restored_zone.name);
            assert_eq!(orig_zone.fans.len(), restored_zone.fans.len());
            for (orig_fan, restored_fan) in orig_zone.fans.iter().zip(restored_zone.fans.iter()) {
                assert_eq!(orig_fan, restored_fan);
            }
            assert_eq!(orig_zone.description, restored_zone.description);
        }
    }

    #[test]
    fn test_empty_zone() {
        let mut data = ZoneData::default();
        data.insert("empty".to_string(), Zone::new("empty", vec![]));

        assert!(data.contains("empty"));
        assert!(data.get("empty").unwrap().fans.is_empty());

        let toml_str = data.to_toml().unwrap();
        let restored = ZoneData::from_toml(&toml_str).unwrap();
        assert!(restored.get("empty").unwrap().fans.is_empty());
    }

    #[test]
    fn test_cross_controller_zone() {
        // Test a zone spanning multiple controllers
        let fans = vec![
            ZoneFan::new("main", 0),
            ZoneFan::new("main", 1),
            ZoneFan::new("gpu", 0),
            ZoneFan::new("gpu", 1),
        ];
        let zone = Zone::with_description("cooling", fans, "All cooling fans");

        assert!(zone.contains_fan("main", 0));
        assert!(zone.contains_fan("main", 1));
        assert!(zone.contains_fan("gpu", 0));
        assert!(zone.contains_fan("gpu", 1));
        assert!(!zone.contains_fan("main", 2));
        assert!(!zone.contains_fan("other", 0));

        let main_fans = zone.fans_for_controller("main");
        assert_eq!(main_fans, vec![0, 1]);

        let gpu_fans = zone.fans_for_controller("gpu");
        assert_eq!(gpu_fans, vec![0, 1]);
    }
}

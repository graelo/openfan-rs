//! Zone data - mutable via API
//!
//! Stored in `{data_dir}/zones.toml`
//!
//! Zones group multiple fan ports for coordinated control.
//! Each port can belong to at most one zone (exclusive membership).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A zone grouping multiple fan ports for coordinated control.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Zone {
    /// Human-readable zone name
    pub name: String,
    /// Fan port IDs belonging to this zone
    pub port_ids: Vec<u8>,
    /// Optional description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Zone {
    /// Create a new zone with the given name and port IDs.
    pub fn new(name: impl Into<String>, port_ids: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            port_ids,
            description: None,
        }
    }

    /// Create a new zone with description.
    pub fn with_description(
        name: impl Into<String>,
        port_ids: Vec<u8>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            port_ids,
            description: Some(description.into()),
        }
    }
}

/// Zone data stored in zones.toml
///
/// Maps zone names to their definitions.
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

    /// Find which zone contains a given port ID.
    ///
    /// Returns the zone name if the port is in a zone, None otherwise.
    pub fn find_zone_for_port(&self, port_id: u8) -> Option<&str> {
        for (name, zone) in &self.zones {
            if zone.port_ids.contains(&port_id) {
                return Some(name);
            }
        }
        None
    }

    /// Check if a port is already assigned to any zone.
    pub fn is_port_assigned(&self, port_id: u8) -> bool {
        self.find_zone_for_port(port_id).is_some()
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
    fn test_zone_creation() {
        let zone = Zone::new("intake", vec![0, 1, 2]);
        assert_eq!(zone.name, "intake");
        assert_eq!(zone.port_ids, vec![0, 1, 2]);
        assert!(zone.description.is_none());

        let zone_with_desc = Zone::with_description("exhaust", vec![3, 4], "Rear exhaust fans");
        assert_eq!(zone_with_desc.name, "exhaust");
        assert_eq!(
            zone_with_desc.description,
            Some("Rear exhaust fans".to_string())
        );
    }

    #[test]
    fn test_zone_operations() {
        let mut data = ZoneData::default();

        let zone = Zone::new("intake", vec![0, 1, 2]);
        data.insert("intake".to_string(), zone);

        assert!(data.contains("intake"));
        assert_eq!(data.get("intake").unwrap().port_ids, vec![0, 1, 2]);

        let removed = data.remove("intake");
        assert!(removed.is_some());
        assert!(!data.contains("intake"));
    }

    #[test]
    fn test_find_zone_for_port() {
        let mut data = ZoneData::default();

        data.insert("intake".to_string(), Zone::new("intake", vec![0, 1, 2]));
        data.insert("exhaust".to_string(), Zone::new("exhaust", vec![3, 4]));

        assert_eq!(data.find_zone_for_port(0), Some("intake"));
        assert_eq!(data.find_zone_for_port(1), Some("intake"));
        assert_eq!(data.find_zone_for_port(3), Some("exhaust"));
        assert_eq!(data.find_zone_for_port(5), None);

        assert!(data.is_port_assigned(0));
        assert!(!data.is_port_assigned(5));
    }

    #[test]
    fn test_zone_serialization() {
        let mut data = ZoneData::default();
        data.insert(
            "intake".to_string(),
            Zone::with_description("intake", vec![0, 1, 2], "Front intake fans"),
        );
        data.insert("exhaust".to_string(), Zone::new("exhaust", vec![3, 4]));

        let toml_str = data.to_toml().unwrap();

        assert!(toml_str.contains("[zones.intake]"));
        assert!(toml_str.contains("[zones.exhaust]"));
        // TOML may serialize arrays with or without spaces
        assert!(
            toml_str.contains("port_ids = [0, 1, 2]")
                || toml_str.contains("port_ids = [\n")
                || toml_str.contains("port_ids = ["),
            "Expected port_ids array in TOML output: {}",
            toml_str
        );
        assert!(toml_str.contains("description = \"Front intake fans\""));
    }

    #[test]
    fn test_zone_deserialization() {
        let toml_str = r#"
            [zones.intake]
            name = "intake"
            port_ids = [0, 1, 2]
            description = "Front intake fans"

            [zones.exhaust]
            name = "exhaust"
            port_ids = [3, 4]
        "#;

        let data = ZoneData::from_toml(toml_str).unwrap();
        assert_eq!(data.zones.len(), 2);

        let intake = data.get("intake").unwrap();
        assert_eq!(intake.name, "intake");
        assert_eq!(intake.port_ids, vec![0, 1, 2]);
        assert_eq!(intake.description, Some("Front intake fans".to_string()));

        let exhaust = data.get("exhaust").unwrap();
        assert_eq!(exhaust.name, "exhaust");
        assert_eq!(exhaust.port_ids, vec![3, 4]);
        assert!(exhaust.description.is_none());
    }

    #[test]
    fn test_zone_roundtrip() {
        let mut original = ZoneData::default();
        original.insert(
            "intake".to_string(),
            Zone::with_description("intake", vec![0, 1, 2], "Front fans"),
        );
        original.insert("exhaust".to_string(), Zone::new("exhaust", vec![3, 4]));

        let toml_str = original.to_toml().unwrap();
        let restored = ZoneData::from_toml(&toml_str).unwrap();

        assert_eq!(original.zones.len(), restored.zones.len());
        for name in original.names() {
            let orig_zone = original.get(name).unwrap();
            let restored_zone = restored.get(name).unwrap();
            assert_eq!(orig_zone.name, restored_zone.name);
            assert_eq!(orig_zone.port_ids, restored_zone.port_ids);
            assert_eq!(orig_zone.description, restored_zone.description);
        }
    }

    #[test]
    fn test_empty_zone() {
        let mut data = ZoneData::default();
        data.insert("empty".to_string(), Zone::new("empty", vec![]));

        assert!(data.contains("empty"));
        assert!(data.get("empty").unwrap().port_ids.is_empty());

        let toml_str = data.to_toml().unwrap();
        let restored = ZoneData::from_toml(&toml_str).unwrap();
        assert!(restored.get("empty").unwrap().port_ids.is_empty());
    }
}

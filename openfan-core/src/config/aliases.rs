//! Fan alias data - mutable via API
//!
//! Stored in `{data_dir}/aliases.toml`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::board::MAX_FANS;

/// Fan alias data stored in aliases.toml
///
/// Maps fan IDs to human-readable names.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasData {
    /// Fan ID to alias name mapping
    ///
    /// Keys are stringified fan IDs ("0", "1", etc.) for TOML compatibility.
    #[serde(
        serialize_with = "serialize_aliases",
        deserialize_with = "deserialize_aliases",
        default
    )]
    pub aliases: HashMap<u8, String>,
}

impl Default for AliasData {
    fn default() -> Self {
        let mut aliases = HashMap::new();
        for i in 0..MAX_FANS as u8 {
            aliases.insert(i, format!("Fan #{}", i + 1));
        }
        Self { aliases }
    }
}

impl AliasData {
    /// Create empty alias data (no defaults).
    pub fn empty() -> Self {
        Self {
            aliases: HashMap::new(),
        }
    }

    /// Get alias for a fan ID, returning a default if not set.
    pub fn get(&self, fan_id: u8) -> String {
        self.aliases
            .get(&fan_id)
            .cloned()
            .unwrap_or_else(|| format!("Fan #{}", fan_id + 1))
    }

    /// Set alias for a fan ID.
    pub fn set(&mut self, fan_id: u8, alias: String) {
        self.aliases.insert(fan_id, alias);
    }

    /// Remove alias for a fan ID (reverts to default).
    ///
    /// Returns `true` if an alias was removed, `false` if none existed.
    pub fn remove(&mut self, fan_id: u8) -> bool {
        self.aliases.remove(&fan_id).is_some()
    }

    /// Parse AliasData from TOML string.
    pub fn from_toml(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }

    /// Serialize AliasData to TOML string.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }
}

// Custom serialization: HashMap<u8, String> -> HashMap<String, String> for TOML
fn serialize_aliases<S>(aliases: &HashMap<u8, String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(Some(aliases.len()))?;
    for (k, v) in aliases {
        map.serialize_entry(&k.to_string(), v)?;
    }
    map.end()
}

// Custom deserialization: HashMap<String, String> -> HashMap<u8, String>
fn deserialize_aliases<'de, D>(deserializer: D) -> Result<HashMap<u8, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let string_map: HashMap<String, String> = HashMap::deserialize(deserializer)?;

    string_map
        .into_iter()
        .map(|(k, v)| {
            k.parse::<u8>()
                .map(|id| (id, v))
                .map_err(|_| D::Error::custom(format!("invalid fan ID: {}", k)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_aliases() {
        let data = AliasData::default();
        assert_eq!(data.aliases.len(), MAX_FANS);
        assert_eq!(data.get(0), "Fan #1");
        assert_eq!(data.get(9), "Fan #10");
    }

    #[test]
    fn test_alias_get_with_default() {
        let data = AliasData::empty();
        // Should return default even if not set
        assert_eq!(data.get(5), "Fan #6");
    }

    #[test]
    fn test_alias_serialization() {
        let mut data = AliasData::empty();
        data.set(0, "CPU Intake".to_string());
        data.set(1, "GPU Exhaust".to_string());

        let toml_str = data.to_toml().unwrap();
        assert!(toml_str.contains("[aliases]"));
        assert!(toml_str.contains("CPU Intake"));
        assert!(toml_str.contains("GPU Exhaust"));
    }

    #[test]
    fn test_alias_deserialization() {
        let toml_str = r#"
            [aliases]
            0 = "CPU Intake"
            1 = "GPU Exhaust"
            5 = "Case Top"
        "#;

        let data = AliasData::from_toml(toml_str).unwrap();
        assert_eq!(data.aliases.get(&0), Some(&"CPU Intake".to_string()));
        assert_eq!(data.aliases.get(&1), Some(&"GPU Exhaust".to_string()));
        assert_eq!(data.aliases.get(&5), Some(&"Case Top".to_string()));
        assert_eq!(data.aliases.get(&2), None); // Not set
    }

    #[test]
    fn test_alias_roundtrip() {
        let original = AliasData::default();
        let toml_str = original.to_toml().unwrap();
        let restored = AliasData::from_toml(&toml_str).unwrap();

        assert_eq!(original.aliases.len(), restored.aliases.len());
        for (id, alias) in &original.aliases {
            assert_eq!(restored.aliases.get(id), Some(alias));
        }
    }

    #[test]
    fn test_alias_remove() {
        let mut data = AliasData::empty();
        data.set(0, "CPU Intake".to_string());
        data.set(1, "GPU Exhaust".to_string());

        // Verify aliases are set
        assert_eq!(data.aliases.get(&0), Some(&"CPU Intake".to_string()));
        assert_eq!(data.aliases.get(&1), Some(&"GPU Exhaust".to_string()));

        // Remove alias for fan 0
        let removed = data.remove(0);
        assert!(removed, "remove() should return true when alias existed");

        // Verify alias is removed (get() returns default)
        assert_eq!(data.aliases.get(&0), None);
        assert_eq!(data.get(0), "Fan #1"); // Default

        // Fan 1 should still have its alias
        assert_eq!(data.aliases.get(&1), Some(&"GPU Exhaust".to_string()));

        // Removing non-existent alias returns false
        let removed_again = data.remove(0);
        assert!(
            !removed_again,
            "remove() should return false when alias didn't exist"
        );

        // Removing never-set alias returns false
        let removed_never_set = data.remove(5);
        assert!(
            !removed_never_set,
            "remove() should return false for never-set alias"
        );
    }
}

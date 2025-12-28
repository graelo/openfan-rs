//! Configuration management module
//!
//! Handles loading, saving, and managing the YAML configuration file.

use openfan_core::{BoardConfig, BoardInfo, Config, DefaultBoard, OpenFanError, Result};
use std::cmp::Ordering;
use std::collections::hash_map::Entry;
use std::path::Path;
use tokio::fs;
use tracing::{debug, info, warn};

/// Configuration validation errors
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    /// Profile has too many fan values for the detected board
    #[error("Profile '{profile_name}' has {config_fans} values but board '{board_name}' only supports {board_fans} fans")]
    TooManyFans {
        profile_name: String,
        config_fans: usize,
        board_fans: usize,
        board_name: String,
    },
    /// Profile has too few fan values (will be auto-filled)
    #[error("Profile '{profile_name}' has {config_fans} values but board '{board_name}' supports {board_fans} fans")]
    TooFewFans {
        profile_name: String,
        config_fans: usize,
        board_fans: usize,
        board_name: String,
    },
    /// Fan alias ID exceeds board's fan count
    #[error("Invalid alias for fan {fan_id} (board '{board_name}' only has {board_fans} fans, max ID is {max_allowed})")]
    InvalidAliasId {
        fan_id: u8,
        max_allowed: u8,
        board_fans: usize,
        board_name: String,
    },
}

/// Configuration manager
pub struct ConfigManager {
    path: std::path::PathBuf,
    config: Config,
}

impl ConfigManager {
    /// Create a new configuration manager
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            config: Config::default(),
        }
    }

    /// Load configuration from file
    ///
    /// If the file doesn't exist, creates it with default values.
    /// If the file exists but is incomplete, merges with defaults and saves.
    pub async fn load(&mut self) -> Result<()> {
        debug!("Loading configuration from: {}", self.path.display());

        // Check if config file exists
        if !self.path.exists() {
            info!(
                "Configuration file not found: {}. Creating with defaults.",
                self.path.display()
            );
            return self.init_default_config().await;
        }

        // Read file contents
        let contents = fs::read_to_string(&self.path)
            .await
            .map_err(|e| OpenFanError::Config(format!("Failed to read config file: {}", e)))?;

        // Parse YAML
        match serde_yaml::from_str::<Config>(&contents) {
            Ok(loaded_config) => {
                self.config = loaded_config;

                // Validate and fill missing data
                let needs_save = self.validate_and_fill_defaults();

                if needs_save {
                    info!("Configuration loaded with missing values. Filling with defaults and saving.");
                    self.save().await?;
                } else {
                    info!("Configuration loaded successfully");
                }

                self.debug_config();
                Ok(())
            }
            Err(e) => {
                info!("Configuration file parse failed: {}. Using defaults.", e);
                self.config = Config::default();
                self.init_default_config().await
            }
        }
    }

    /// Save current configuration to file
    pub async fn save(&self) -> Result<()> {
        debug!("Saving configuration to: {}", self.path.display());

        let yaml = serde_yaml::to_string(&self.config)
            .map_err(|e| OpenFanError::Config(format!("Failed to serialize config: {}", e)))?;

        fs::write(&self.path, yaml)
            .await
            .map_err(|e| OpenFanError::Config(format!("Failed to write config file: {}", e)))?;

        info!("Configuration saved successfully");
        Ok(())
    }

    /// Initialize with default configuration and save to file
    async fn init_default_config(&mut self) -> Result<()> {
        info!("Initializing default configuration");
        self.config = Config::default();
        self.save().await?;
        Ok(())
    }

    /// Validate configuration and fill missing values with defaults
    ///
    /// Returns true if any values were filled
    fn validate_and_fill_defaults(&mut self) -> bool {
        let mut modified = false;

        // Ensure we have at least the default profiles
        let default_config = Config::default();

        if self.config.fan_profiles.is_empty() {
            debug!("No fan profiles found, adding defaults");
            self.config.fan_profiles = default_config.fan_profiles;
            modified = true;
        }

        // Ensure we have all fan aliases
        for i in 0..DefaultBoard::FAN_COUNT as u8 {
            if let Entry::Vacant(e) = self.config.fan_aliases.entry(i) {
                debug!("Missing alias for fan {}, adding default", i);
                e.insert(format!("Fan #{}", i + 1));
                modified = true;
            }
        }

        modified
    }

    /// Print configuration to debug log
    fn debug_config(&self) {
        debug!("--- Server Config ---");
        debug!("  Host: {}", self.config.server.hostname);
        debug!("  Port: {}", self.config.server.port);
        debug!("  Timeout: {}s", self.config.server.communication_timeout);
        debug!("--- Hardware Config ---");
        debug!("  Host: {}", self.config.hardware.hostname);
        debug!("  Port: {}", self.config.hardware.port);
        debug!("  Timeout: {}s", self.config.hardware.communication_timeout);
        debug!("--- Fan Profiles ---");
        for (name, profile) in &self.config.fan_profiles {
            debug!(
                "  {}: {:?} - {:?}",
                name, profile.control_mode, profile.values
            );
        }
        debug!("--- Fan Aliases ---");
        for i in 0..DefaultBoard::FAN_COUNT as u8 {
            if let Some(alias) = self.config.fan_aliases.get(&i) {
                debug!("  Fan {}: {}", i, alias);
            }
        }
        debug!("--------------------");
    }

    /// Validate configuration against detected board
    ///
    /// Checks that:
    /// - All fan profiles have the correct number of values for the board
    /// - All fan aliases are within the valid range for the board
    ///
    /// # Errors
    ///
    /// Returns validation errors if:
    /// - Any profile has too many fans (hard error)
    /// - Any profile has too few fans (warning, will auto-fill)
    /// - Any alias ID is out of range for the board
    pub fn validate_for_board(
        &self,
        board: &BoardInfo,
    ) -> std::result::Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Validate fan profiles
        for (name, profile) in &self.config.fan_profiles {
            match profile.values.len().cmp(&board.fan_count) {
                Ordering::Greater => {
                    errors.push(ValidationError::TooManyFans {
                        profile_name: name.clone(),
                        config_fans: profile.values.len(),
                        board_fans: board.fan_count,
                        board_name: board.name.to_string(),
                    });
                }
                Ordering::Less => {
                    warnings.push(ValidationError::TooFewFans {
                        profile_name: name.clone(),
                        config_fans: profile.values.len(),
                        board_fans: board.fan_count,
                        board_name: board.name.to_string(),
                    });
                }
                Ordering::Equal => {}
            }
        }

        // Validate fan aliases
        if let Some(&max_id) = self.config.fan_aliases.keys().max() {
            if max_id >= board.fan_count as u8 {
                errors.push(ValidationError::InvalidAliasId {
                    fan_id: max_id,
                    max_allowed: (board.fan_count - 1) as u8,
                    board_fans: board.fan_count,
                    board_name: board.name.to_string(),
                });
            }
        }

        // Log warnings (non-fatal)
        for warning in warnings {
            warn!("{}", warning);
        }

        // Return errors if any
        if !errors.is_empty() {
            Err(errors)
        } else {
            Ok(())
        }
    }

    /// Fill missing defaults for the detected board
    ///
    /// Ensures all fan aliases exist for the board's fan count.
    /// This is called after validation to auto-fill partial configs.
    pub async fn fill_defaults_for_board(&mut self, board: &BoardInfo) -> Result<()> {
        let mut modified = false;

        // Add missing fan aliases
        for i in 0..board.fan_count as u8 {
            if let Entry::Vacant(e) = self.config.fan_aliases.entry(i) {
                debug!("Adding missing alias for fan {}", i);
                e.insert(format!("Fan #{}", i + 1));
                modified = true;
            }
        }

        if modified {
            self.save().await?;
            info!(
                "Configuration updated with missing defaults for {}",
                board.name
            );
        }

        Ok(())
    }

    /// Get immutable reference to configuration
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Get mutable reference to configuration
    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_config_manager_default() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        // Remove the file so we test creation
        std::fs::remove_file(path).unwrap();

        let mut manager = ConfigManager::new(path);
        manager.load().await.unwrap();

        // Check defaults
        assert_eq!(manager.config().server.port, 3000);
        assert_eq!(manager.config().fan_profiles.len(), 3);
        assert_eq!(manager.config().fan_aliases.len(), 10);

        // File should now exist
        assert!(path.exists());
    }

    #[tokio::test]
    async fn test_config_manager_load_save() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        let mut manager = ConfigManager::new(path);
        manager.load().await.unwrap();

        // Modify config
        manager.config_mut().server.port = 4000;
        manager.save().await.unwrap();

        // Load again and verify
        let mut manager2 = ConfigManager::new(path);
        manager2.load().await.unwrap();
        assert_eq!(manager2.config().server.port, 4000);
    }

    #[tokio::test]
    async fn test_config_validation() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        // Create incomplete config
        let incomplete_yaml = r#"
server:
  hostname: localhost
  port: 3000
  communication_timeout: 1
hardware:
  hostname: localhost
  port: 3000
  communication_timeout: 1
fan_profiles: {}
fan_aliases: {}
"#;
        std::fs::write(path, incomplete_yaml).unwrap();

        let mut manager = ConfigManager::new(path);
        manager.load().await.unwrap();

        // Should have filled defaults
        assert!(!manager.config().fan_profiles.is_empty());
        assert_eq!(manager.config().fan_aliases.len(), 10);
    }
}

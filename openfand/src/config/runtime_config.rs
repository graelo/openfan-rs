//! Runtime configuration management
//!
//! Combines static configuration with mutable data files, providing
//! thread-safe access and independent save operations.

use openfan_core::{
    config::{AliasData, CfmMappingData, ProfileData, StaticConfig, ThermalCurveData, ZoneData},
    BoardInfo, OpenFanError, Result,
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::ControllerData;

/// Runtime configuration combining static config and mutable data.
///
/// Static config is read once at startup and remains immutable.
/// Mutable data (aliases, profiles, zones, thermal curves, cfm mappings) can be modified via API and saved independently.
///
/// For multi-controller setups, per-controller data is stored separately in `ControllerData`
/// instances accessed via `controller_data()`.
pub(crate) struct RuntimeConfig {
    /// Static configuration (immutable after load)
    static_config: StaticConfig,

    /// Per-controller mutable data (aliases, profiles, curves, CFM)
    /// Key is controller ID, value is the controller's data
    controller_data: RwLock<HashMap<String, Arc<ControllerData>>>,

    // Global data (used by default controller and legacy single-controller mode)
    /// Alias data with independent locking
    aliases: RwLock<AliasData>,

    /// Profile data with independent locking
    profiles: RwLock<ProfileData>,

    /// Zone data with independent locking (zones are cross-controller)
    zones: RwLock<ZoneData>,

    /// CFM mapping data with independent locking
    cfm_mappings: RwLock<CfmMappingData>,
}

impl RuntimeConfig {
    /// Load all configuration from disk.
    ///
    /// If config file doesn't exist, creates with defaults.
    /// If data directory doesn't exist, creates it.
    /// If data files don't exist, creates with defaults.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use std::path::Path;
    ///
    /// let config = RuntimeConfig::load(Path::new("/etc/openfan/config.toml")).await?;
    /// println!("Data directory: {}", config.data_dir().display());
    /// ```
    pub async fn load(config_path: &Path) -> Result<Self> {
        info!("Loading configuration from: {}", config_path.display());

        // Load or create static config
        let static_config = Self::load_static_config(config_path).await?;

        // Ensure data directory exists
        Self::ensure_data_dir(&static_config.data_dir).await?;

        // Load or create mutable data files
        let aliases = Self::load_aliases(&static_config.data_dir).await?;
        let profiles = Self::load_profiles(&static_config.data_dir).await?;
        let zones = Self::load_zones(&static_config.data_dir).await?;
        let cfm_mappings = Self::load_cfm_mappings(&static_config.data_dir).await?;

        // Ensure thermal_curves.toml exists (for per-controller data compatibility)
        Self::ensure_thermal_curves_file(&static_config.data_dir).await?;

        info!(
            "Configuration loaded: {} profiles, {} aliases, {} zones, {} CFM mappings",
            profiles.profiles.len(),
            aliases.aliases.len(),
            zones.zones.len(),
            cfm_mappings.len()
        );

        Ok(Self {
            static_config,
            controller_data: RwLock::new(HashMap::new()),
            aliases: RwLock::new(aliases),
            profiles: RwLock::new(profiles),
            zones: RwLock::new(zones),
            cfm_mappings: RwLock::new(cfm_mappings),
        })
    }

    /// Load static config from TOML file, creating with defaults if missing.
    async fn load_static_config(path: &Path) -> Result<StaticConfig> {
        if !path.exists() {
            info!(
                "Static config not found at {}. Creating with defaults.",
                path.display()
            );

            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).await.map_err(|e| {
                    OpenFanError::Config(format!(
                        "Failed to create config directory '{}': {}",
                        parent.display(),
                        e
                    ))
                })?;
            }

            let config = StaticConfig::default();
            let toml_str = config
                .to_toml()
                .map_err(|e| OpenFanError::Config(format!("Failed to serialize config: {}", e)))?;

            fs::write(path, &toml_str)
                .await
                .map_err(|e| OpenFanError::Config(format!("Failed to write config file: {}", e)))?;

            return Ok(config);
        }

        let content = fs::read_to_string(path)
            .await
            .map_err(|e| OpenFanError::Config(format!("Failed to read config file: {}", e)))?;

        StaticConfig::from_toml(&content)
            .map_err(|e| OpenFanError::Config(format!("Failed to parse config file: {}", e)))
    }

    /// Ensure data directory exists and is writable.
    async fn ensure_data_dir(data_dir: &Path) -> Result<()> {
        if !data_dir.exists() {
            info!("Creating data directory: {}", data_dir.display());
            fs::create_dir_all(data_dir).await.map_err(|e| {
                OpenFanError::Config(format!(
                    "Failed to create data directory '{}': {}. \
                     Please create it manually or check permissions.",
                    data_dir.display(),
                    e
                ))
            })?;
        }

        // Verify we can write to it
        let test_file = data_dir.join(".write_test");
        fs::write(&test_file, "test").await.map_err(|e| {
            OpenFanError::Config(format!(
                "Data directory '{}' is not writable: {}",
                data_dir.display(),
                e
            ))
        })?;
        let _ = fs::remove_file(&test_file).await;

        Ok(())
    }

    /// Load aliases from TOML file, creating with defaults if missing.
    async fn load_aliases(data_dir: &Path) -> Result<AliasData> {
        let path = data_dir.join("aliases.toml");

        if !path.exists() {
            debug!("Aliases file not found. Creating with defaults.");
            let data = AliasData::default();
            Self::write_toml(&path, &data.to_toml().unwrap()).await?;
            return Ok(data);
        }

        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| OpenFanError::Config(format!("Failed to read aliases file: {}", e)))?;

        AliasData::from_toml(&content)
            .map_err(|e| OpenFanError::Config(format!("Failed to parse aliases file: {}", e)))
    }

    /// Load profiles from TOML file, creating with defaults if missing.
    async fn load_profiles(data_dir: &Path) -> Result<ProfileData> {
        let path = data_dir.join("profiles.toml");

        if !path.exists() {
            debug!("Profiles file not found. Creating with defaults.");
            let data = ProfileData::with_defaults();
            Self::write_toml(&path, &data.to_toml().unwrap()).await?;
            return Ok(data);
        }

        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| OpenFanError::Config(format!("Failed to read profiles file: {}", e)))?;

        ProfileData::from_toml(&content)
            .map_err(|e| OpenFanError::Config(format!("Failed to parse profiles file: {}", e)))
    }

    /// Load zones from TOML file, creating with defaults if missing.
    async fn load_zones(data_dir: &Path) -> Result<ZoneData> {
        let path = data_dir.join("zones.toml");

        if !path.exists() {
            debug!("Zones file not found. Creating with defaults.");
            let data = ZoneData::default();
            Self::write_toml(&path, &data.to_toml().unwrap()).await?;
            return Ok(data);
        }

        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| OpenFanError::Config(format!("Failed to read zones file: {}", e)))?;

        ZoneData::from_toml(&content)
            .map_err(|e| OpenFanError::Config(format!("Failed to parse zones file: {}", e)))
    }

    /// Ensure thermal curves file exists with defaults (for backward compatibility).
    ///
    /// Thermal curves are now per-controller via ControllerData, but we still
    /// create the global file for migration purposes.
    async fn ensure_thermal_curves_file(data_dir: &Path) -> Result<()> {
        let path = data_dir.join("thermal_curves.toml");

        if !path.exists() {
            debug!("Thermal curves file not found. Creating with defaults.");
            let data = ThermalCurveData::with_defaults();
            Self::write_toml(&path, &data.to_toml().unwrap()).await?;
        }

        Ok(())
    }

    /// Load CFM mappings from TOML file, creating empty if missing.
    async fn load_cfm_mappings(data_dir: &Path) -> Result<CfmMappingData> {
        let path = data_dir.join("cfm_mappings.toml");

        if !path.exists() {
            debug!("CFM mappings file not found. Creating empty.");
            let data = CfmMappingData::default();
            Self::write_toml(&path, &data.to_toml().unwrap()).await?;
            return Ok(data);
        }

        let content = fs::read_to_string(&path).await.map_err(|e| {
            OpenFanError::Config(format!("Failed to read CFM mappings file: {}", e))
        })?;

        CfmMappingData::from_toml(&content)
            .map_err(|e| OpenFanError::Config(format!("Failed to parse CFM mappings file: {}", e)))
    }

    /// Write TOML content atomically (write to temp, then rename).
    async fn write_toml(path: &Path, content: &str) -> Result<()> {
        let temp_path = path.with_extension("toml.tmp");

        fs::write(&temp_path, content)
            .await
            .map_err(|e| OpenFanError::Config(format!("Failed to write temp file: {}", e)))?;

        fs::rename(&temp_path, path)
            .await
            .map_err(|e| OpenFanError::Config(format!("Failed to rename temp file: {}", e)))?;

        Ok(())
    }

    // =========================================================================
    // Static config access (read-only)
    // =========================================================================

    /// Get reference to static configuration.
    pub fn static_config(&self) -> &StaticConfig {
        &self.static_config
    }

    /// Get data directory path.
    pub fn data_dir(&self) -> &Path {
        &self.static_config.data_dir
    }

    // =========================================================================
    // Per-controller data access
    // =========================================================================

    /// Get or create controller data for the specified controller.
    ///
    /// If the controller data doesn't exist yet, it will be loaded from disk
    /// (or created with defaults if the files don't exist).
    ///
    /// # Arguments
    ///
    /// * `controller_id` - Unique identifier for the controller
    ///
    /// # Example
    ///
    /// ```ignore
    /// let ctrl_data = config.controller_data("main").await?;
    ///
    /// // Access controller-specific profiles
    /// let profiles = ctrl_data.profiles().await;
    /// for profile in profiles.profiles.values() {
    ///     println!("Profile: {}", profile.name);
    /// }
    /// ```
    pub async fn controller_data(&self, controller_id: &str) -> Result<Arc<ControllerData>> {
        // First try to get existing data with read lock
        {
            let data = self.controller_data.read().await;
            if let Some(cd) = data.get(controller_id) {
                return Ok(cd.clone());
            }
        }

        // Need to create new controller data
        let cd = ControllerData::load(controller_id, self.data_dir()).await?;
        let cd = Arc::new(cd);

        // Store in cache
        {
            let mut data = self.controller_data.write().await;
            // Check again in case another task created it while we were loading
            if let Some(existing) = data.get(controller_id) {
                return Ok(existing.clone());
            }
            data.insert(controller_id.to_string(), cd.clone());
        }

        Ok(cd)
    }

    // =========================================================================
    // Profile access (used by shutdown handler)
    // =========================================================================

    /// Get read lock on profile data.
    pub async fn profiles(&self) -> tokio::sync::RwLockReadGuard<'_, ProfileData> {
        self.profiles.read().await
    }

    // =========================================================================
    // Zone access and modification (zones are global, cross-controller)
    // =========================================================================

    /// Get read lock on zone data.
    pub async fn zones(&self) -> tokio::sync::RwLockReadGuard<'_, ZoneData> {
        self.zones.read().await
    }

    /// Get write lock on zone data.
    pub async fn zones_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, ZoneData> {
        self.zones.write().await
    }

    /// Save zone data to disk.
    pub async fn save_zones(&self) -> Result<()> {
        let zones = self.zones.read().await;
        let path = self.static_config.data_dir.join("zones.toml");

        let content = zones
            .to_toml()
            .map_err(|e| OpenFanError::Config(format!("Failed to serialize zones: {}", e)))?;

        Self::write_toml(&path, &content).await?;

        debug!("Saved zones to {}", path.display());
        Ok(())
    }

    // =========================================================================
    // Internal save methods
    // =========================================================================

    /// Save alias data to disk (used internally by fill_defaults_for_board).
    async fn save_aliases(&self) -> Result<()> {
        let aliases = self.aliases.read().await;
        let path = self.static_config.data_dir.join("aliases.toml");

        let content = aliases
            .to_toml()
            .map_err(|e| OpenFanError::Config(format!("Failed to serialize aliases: {}", e)))?;

        Self::write_toml(&path, &content).await?;

        debug!("Saved aliases to {}", path.display());
        Ok(())
    }

    // =========================================================================
    // Board validation
    // =========================================================================

    /// Validate configuration against detected board.
    ///
    /// Checks that profiles, aliases, zones, and CFM mappings are compatible with the board's fan count.
    pub async fn validate_for_board(&self, board: &BoardInfo) -> Result<()> {
        let profiles = self.profiles.read().await;
        let aliases = self.aliases.read().await;
        let zones = self.zones.read().await;
        let cfm_mappings = self.cfm_mappings.read().await;

        // Validate profiles
        for (name, profile) in &profiles.profiles {
            if profile.values.len() > board.fan_count {
                return Err(OpenFanError::Config(format!(
                    "Profile '{}' has {} values but board '{}' only supports {} fans",
                    name,
                    profile.values.len(),
                    board.name,
                    board.fan_count
                )));
            }
            if profile.values.len() < board.fan_count {
                warn!(
                    "Profile '{}' has {} values but board has {} fans (will use defaults for extra fans)",
                    name, profile.values.len(), board.fan_count
                );
            }
        }

        // Validate aliases
        if let Some(&max_id) = aliases.aliases.keys().max() {
            if max_id >= board.fan_count as u8 {
                return Err(OpenFanError::Config(format!(
                    "Alias exists for fan {} but board '{}' only has {} fans (max ID: {})",
                    max_id,
                    board.name,
                    board.fan_count,
                    board.fan_count - 1
                )));
            }
        }

        // Validate zones
        // TODO: In multi-controller mode, validate against each controller's board info
        for (name, zone) in &zones.zones {
            for fan in &zone.fans {
                if fan.fan_id >= board.fan_count as u8 {
                    return Err(OpenFanError::Config(format!(
                        "Zone '{}' references fan {} (controller: '{}') but board '{}' only has {} fans (max ID: {})",
                        name,
                        fan.fan_id,
                        fan.controller,
                        board.name,
                        board.fan_count,
                        board.fan_count - 1
                    )));
                }
            }
        }

        // Validate CFM mappings
        if let Some(&max_port) = cfm_mappings.mappings.keys().max() {
            if max_port >= board.fan_count as u8 {
                return Err(OpenFanError::Config(format!(
                    "CFM mapping exists for port {} but board '{}' only has {} fans (max ID: {})",
                    max_port,
                    board.name,
                    board.fan_count,
                    board.fan_count - 1
                )));
            }
        }

        Ok(())
    }

    /// Fill missing defaults for the detected board.
    ///
    /// Ensures aliases exist for all fans on the board.
    pub async fn fill_defaults_for_board(&self, board: &BoardInfo) -> Result<()> {
        let mut modified = false;

        {
            let mut aliases = self.aliases.write().await;
            for i in 0..board.fan_count as u8 {
                if !aliases.aliases.contains_key(&i) {
                    debug!("Adding missing alias for fan {}", i);
                    aliases.set(i, format!("Fan #{}", i + 1));
                    modified = true;
                }
            }
        }

        if modified {
            self.save_aliases().await?;
            info!("Added missing default aliases for {}", board.name);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    async fn create_test_config(dir: &Path) -> PathBuf {
        let config_path = dir.join("config.toml");
        let config = StaticConfig::with_data_dir(dir.join("data"));
        fs::write(&config_path, config.to_toml().unwrap())
            .await
            .unwrap();
        config_path
    }

    #[tokio::test]
    async fn test_runtime_config_load_creates_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let config = RuntimeConfig::load(&config_path).await.unwrap();

        // Should have created config file
        assert!(config_path.exists());

        // Should have created data directory with default files
        assert!(config.data_dir().exists());
        assert!(config.data_dir().join("aliases.toml").exists());
        assert!(config.data_dir().join("profiles.toml").exists());
        assert!(config.data_dir().join("zones.toml").exists());
        assert!(config.data_dir().join("thermal_curves.toml").exists());
        assert!(config.data_dir().join("cfm_mappings.toml").exists());

        // Should have default profiles
        let profiles = config.profiles().await;
        assert!(profiles.contains("50% PWM"));
        assert!(profiles.contains("100% PWM"));
        assert!(profiles.contains("1000 RPM"));
    }

    #[tokio::test]
    async fn test_runtime_config_zone_operations() {
        use openfan_core::ZoneFan;

        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config(temp_dir.path()).await;

        let config = RuntimeConfig::load(&config_path).await.unwrap();

        // Add a zone with cross-controller fans
        let fans = vec![
            ZoneFan::new("default", 0),
            ZoneFan::new("default", 1),
            ZoneFan::new("default", 2),
        ];
        {
            let mut zones = config.zones_mut().await;
            zones.insert(
                "intake".to_string(),
                openfan_core::Zone::with_description("intake", fans, "Front intake fans"),
            );
        }

        // Save
        config.save_zones().await.unwrap();

        // Reload and verify
        let config2 = RuntimeConfig::load(&config_path).await.unwrap();
        let zones = config2.zones().await;
        assert!(zones.contains("intake"));
        let intake = zones.get("intake").unwrap();
        assert_eq!(intake.fans.len(), 3);
        assert_eq!(intake.fans[0].controller, "default");
        assert_eq!(intake.fans[0].fan_id, 0);
        assert_eq!(intake.description, Some("Front intake fans".to_string()));
    }

    #[tokio::test]
    async fn test_validate_for_board_valid_zone() {
        use openfan_core::board::BoardType;
        use openfan_core::ZoneFan;

        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config(temp_dir.path()).await;
        let config = RuntimeConfig::load(&config_path).await.unwrap();

        // Standard board has 10 fans
        let board = BoardType::OpenFanStandard.to_board_info();

        // Add valid zone (all fans within 10-fan limit)
        {
            let fans = vec![
                ZoneFan::new("default", 0),
                ZoneFan::new("default", 1),
                ZoneFan::new("default", 2),
            ];
            let mut zones = config.zones_mut().await;
            zones.insert(
                "intake".to_string(),
                openfan_core::Zone::new("intake", fans),
            );
        }

        // Validation should pass
        assert!(config.validate_for_board(&board).await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_for_board_invalid_zone() {
        use openfan_core::board::BoardType;
        use openfan_core::ZoneFan;

        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config(temp_dir.path()).await;
        let config = RuntimeConfig::load(&config_path).await.unwrap();

        // Standard board has 10 fans (IDs 0-9)
        let board = BoardType::OpenFanStandard.to_board_info();

        // Add zone referencing fan 15 (invalid)
        let fans = vec![ZoneFan::new("default", 0), ZoneFan::new("default", 15)];
        {
            let mut zones = config.zones_mut().await;
            zones.insert(
                "invalid".to_string(),
                openfan_core::Zone::new("invalid", fans),
            );
        }

        // Validation should fail
        let result = config.validate_for_board(&board).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Zone 'invalid' references fan 15"));
    }
}

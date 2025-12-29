//! Runtime configuration management
//!
//! Combines static configuration with mutable data files, providing
//! thread-safe access and independent save operations.

use openfan_core::{
    config::{AliasData, ProfileData, StaticConfig, ThermalCurveData, ZoneData},
    BoardInfo, OpenFanError, Result,
};
use std::path::Path;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Runtime configuration combining static config and mutable data.
///
/// Static config is read once at startup and remains immutable.
/// Mutable data (aliases, profiles, zones, thermal curves) can be modified via API and saved independently.
pub(crate) struct RuntimeConfig {
    /// Static configuration (immutable after load)
    static_config: StaticConfig,

    /// Alias data with independent locking
    aliases: RwLock<AliasData>,

    /// Profile data with independent locking
    profiles: RwLock<ProfileData>,

    /// Zone data with independent locking
    zones: RwLock<ZoneData>,

    /// Thermal curve data with independent locking
    thermal_curves: RwLock<ThermalCurveData>,
}

impl RuntimeConfig {
    /// Load all configuration from disk.
    ///
    /// If config file doesn't exist, creates with defaults.
    /// If data directory doesn't exist, creates it.
    /// If data files don't exist, creates with defaults.
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
        let thermal_curves = Self::load_thermal_curves(&static_config.data_dir).await?;

        info!(
            "Configuration loaded: {} profiles, {} aliases, {} zones, {} thermal curves",
            profiles.profiles.len(),
            aliases.aliases.len(),
            zones.zones.len(),
            thermal_curves.curves.len()
        );

        Ok(Self {
            static_config,
            aliases: RwLock::new(aliases),
            profiles: RwLock::new(profiles),
            zones: RwLock::new(zones),
            thermal_curves: RwLock::new(thermal_curves),
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

    /// Load thermal curves from TOML file, creating with defaults if missing.
    async fn load_thermal_curves(data_dir: &Path) -> Result<ThermalCurveData> {
        let path = data_dir.join("thermal_curves.toml");

        if !path.exists() {
            debug!("Thermal curves file not found. Creating with defaults.");
            let data = ThermalCurveData::with_defaults();
            Self::write_toml(&path, &data.to_toml().unwrap()).await?;
            return Ok(data);
        }

        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| OpenFanError::Config(format!("Failed to read thermal curves file: {}", e)))?;

        ThermalCurveData::from_toml(&content)
            .map_err(|e| OpenFanError::Config(format!("Failed to parse thermal curves file: {}", e)))
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
    // Alias access and modification
    // =========================================================================

    /// Get read lock on alias data.
    pub async fn aliases(&self) -> tokio::sync::RwLockReadGuard<'_, AliasData> {
        self.aliases.read().await
    }

    /// Get write lock on alias data.
    pub async fn aliases_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, AliasData> {
        self.aliases.write().await
    }

    /// Save alias data to disk.
    pub async fn save_aliases(&self) -> Result<()> {
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
    // Profile access and modification
    // =========================================================================

    /// Get read lock on profile data.
    pub async fn profiles(&self) -> tokio::sync::RwLockReadGuard<'_, ProfileData> {
        self.profiles.read().await
    }

    /// Get write lock on profile data.
    pub async fn profiles_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, ProfileData> {
        self.profiles.write().await
    }

    /// Save profile data to disk.
    pub async fn save_profiles(&self) -> Result<()> {
        let profiles = self.profiles.read().await;
        let path = self.static_config.data_dir.join("profiles.toml");

        let content = profiles
            .to_toml()
            .map_err(|e| OpenFanError::Config(format!("Failed to serialize profiles: {}", e)))?;

        Self::write_toml(&path, &content).await?;

        debug!("Saved profiles to {}", path.display());
        Ok(())
    }

    // =========================================================================
    // Zone access and modification
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
    // Thermal curve access and modification
    // =========================================================================

    /// Get read lock on thermal curve data.
    pub async fn thermal_curves(&self) -> tokio::sync::RwLockReadGuard<'_, ThermalCurveData> {
        self.thermal_curves.read().await
    }

    /// Get write lock on thermal curve data.
    pub async fn thermal_curves_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, ThermalCurveData> {
        self.thermal_curves.write().await
    }

    /// Save thermal curve data to disk.
    pub async fn save_thermal_curves(&self) -> Result<()> {
        let curves = self.thermal_curves.read().await;
        let path = self.static_config.data_dir.join("thermal_curves.toml");

        let content = curves
            .to_toml()
            .map_err(|e| OpenFanError::Config(format!("Failed to serialize thermal curves: {}", e)))?;

        Self::write_toml(&path, &content).await?;

        debug!("Saved thermal curves to {}", path.display());
        Ok(())
    }

    // =========================================================================
    // Board validation
    // =========================================================================

    /// Validate configuration against detected board.
    ///
    /// Checks that profiles, aliases, and zones are compatible with the board's fan count.
    pub async fn validate_for_board(&self, board: &BoardInfo) -> Result<()> {
        let profiles = self.profiles.read().await;
        let aliases = self.aliases.read().await;
        let zones = self.zones.read().await;

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
        for (name, zone) in &zones.zones {
            for &port_id in &zone.port_ids {
                if port_id >= board.fan_count as u8 {
                    return Err(OpenFanError::Config(format!(
                        "Zone '{}' references port {} but board '{}' only has {} fans (max ID: {})",
                        name,
                        port_id,
                        board.name,
                        board.fan_count,
                        board.fan_count - 1
                    )));
                }
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

        // Should have default profiles
        let profiles = config.profiles().await;
        assert!(profiles.contains("50% PWM"));
        assert!(profiles.contains("100% PWM"));
        assert!(profiles.contains("1000 RPM"));

        // Should have default thermal curves
        let curves = config.thermal_curves().await;
        assert!(curves.contains("Balanced"));
        assert!(curves.contains("Silent"));
        assert!(curves.contains("Aggressive"));
    }

    #[tokio::test]
    async fn test_runtime_config_alias_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config(temp_dir.path()).await;

        let config = RuntimeConfig::load(&config_path).await.unwrap();

        // Modify alias
        {
            let mut aliases = config.aliases_mut().await;
            aliases.set(0, "CPU Intake".to_string());
        }

        // Save
        config.save_aliases().await.unwrap();

        // Reload and verify
        let config2 = RuntimeConfig::load(&config_path).await.unwrap();
        let aliases = config2.aliases().await;
        assert_eq!(aliases.get(0), "CPU Intake");
    }

    #[tokio::test]
    async fn test_runtime_config_profile_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config(temp_dir.path()).await;

        let config = RuntimeConfig::load(&config_path).await.unwrap();

        // Add a profile
        {
            let mut profiles = config.profiles_mut().await;
            profiles.insert(
                "Custom".to_string(),
                openfan_core::FanProfile::new(
                    openfan_core::ControlMode::Pwm,
                    vec![42; openfan_core::board::MAX_FANS],
                ),
            );
        }

        // Save
        config.save_profiles().await.unwrap();

        // Reload and verify
        let config2 = RuntimeConfig::load(&config_path).await.unwrap();
        let profiles = config2.profiles().await;
        assert!(profiles.contains("Custom"));
        assert_eq!(profiles.get("Custom").unwrap().values[0], 42);
    }

    #[tokio::test]
    async fn test_runtime_config_zone_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config(temp_dir.path()).await;

        let config = RuntimeConfig::load(&config_path).await.unwrap();

        // Add a zone
        {
            let mut zones = config.zones_mut().await;
            zones.insert(
                "intake".to_string(),
                openfan_core::Zone::with_description("intake", vec![0, 1, 2], "Front intake fans"),
            );
        }

        // Save
        config.save_zones().await.unwrap();

        // Reload and verify
        let config2 = RuntimeConfig::load(&config_path).await.unwrap();
        let zones = config2.zones().await;
        assert!(zones.contains("intake"));
        let intake = zones.get("intake").unwrap();
        assert_eq!(intake.port_ids, vec![0, 1, 2]);
        assert_eq!(intake.description, Some("Front intake fans".to_string()));
    }

    #[tokio::test]
    async fn test_runtime_config_thermal_curve_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config(temp_dir.path()).await;

        let config = RuntimeConfig::load(&config_path).await.unwrap();

        // Should have default curves
        {
            let curves = config.thermal_curves().await;
            assert!(curves.contains("Balanced"));
            assert!(curves.contains("Silent"));
            assert!(curves.contains("Aggressive"));
        }

        // Add a custom curve
        {
            let mut curves = config.thermal_curves_mut().await;
            curves.insert(
                "Custom".to_string(),
                openfan_core::ThermalCurve::with_description(
                    "Custom",
                    vec![
                        openfan_core::CurvePoint::new(25.0, 20),
                        openfan_core::CurvePoint::new(60.0, 60),
                        openfan_core::CurvePoint::new(80.0, 100),
                    ],
                    "Custom test curve",
                ),
            );
        }

        // Save
        config.save_thermal_curves().await.unwrap();

        // Reload and verify
        let config2 = RuntimeConfig::load(&config_path).await.unwrap();
        let curves = config2.thermal_curves().await;
        assert!(curves.contains("Custom"));
        let custom = curves.get("Custom").unwrap();
        assert_eq!(custom.points.len(), 3);
        assert_eq!(custom.description, Some("Custom test curve".to_string()));

        // Test interpolation
        assert_eq!(custom.interpolate(25.0), 20);
        assert_eq!(custom.interpolate(80.0), 100);
    }
}

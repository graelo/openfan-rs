//! Per-controller mutable data management
//!
//! Each controller has its own set of aliases, profiles, thermal curves,
//! and CFM mappings stored in a separate directory under the data directory.

use openfan_core::{
    config::{AliasData, CfmMappingData, ProfileData, ThermalCurveData},
    BoardInfo, OpenFanError, Result,
};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Per-controller mutable data storage
///
/// Manages aliases, profiles, thermal curves, and CFM mappings for a single
/// controller. Each controller has its own data directory.
pub struct ControllerData {
    /// Controller ID
    id: String,

    /// Board information for validation
    board_info: BoardInfo,

    /// Path to this controller's data directory
    data_path: PathBuf,

    /// Alias data with independent locking
    aliases: RwLock<AliasData>,

    /// Profile data with independent locking
    profiles: RwLock<ProfileData>,

    /// Thermal curve data with independent locking
    thermal_curves: RwLock<ThermalCurveData>,

    /// CFM mapping data with independent locking
    cfm_mappings: RwLock<CfmMappingData>,
}

impl ControllerData {
    /// Load or create controller data from a directory
    ///
    /// Creates the directory structure if it doesn't exist.
    pub async fn load(
        id: impl Into<String>,
        board_info: BoardInfo,
        base_data_dir: &Path,
    ) -> Result<Self> {
        let id = id.into();
        let data_path = base_data_dir.join("controllers").join(&id);

        info!(
            "Loading controller data for '{}' from: {}",
            id,
            data_path.display()
        );

        // Ensure controller data directory exists
        Self::ensure_data_dir(&data_path).await?;

        // Load or create mutable data files
        let aliases = Self::load_aliases(&data_path).await?;
        let profiles = Self::load_profiles(&data_path).await?;
        let thermal_curves = Self::load_thermal_curves(&data_path).await?;
        let cfm_mappings = Self::load_cfm_mappings(&data_path).await?;

        info!(
            "Controller '{}' data loaded: {} profiles, {} aliases, {} curves, {} CFM mappings",
            id,
            profiles.profiles.len(),
            aliases.aliases.len(),
            thermal_curves.curves.len(),
            cfm_mappings.len()
        );

        Ok(Self {
            id,
            board_info,
            data_path,
            aliases: RwLock::new(aliases),
            profiles: RwLock::new(profiles),
            thermal_curves: RwLock::new(thermal_curves),
            cfm_mappings: RwLock::new(cfm_mappings),
        })
    }

    /// Get the controller ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get the board info for this controller
    pub fn board_info(&self) -> &BoardInfo {
        &self.board_info
    }

    /// Get the data directory path
    pub fn data_path(&self) -> &Path {
        &self.data_path
    }

    /// Ensure data directory exists and is writable
    async fn ensure_data_dir(data_path: &Path) -> Result<()> {
        if !data_path.exists() {
            info!(
                "Creating controller data directory: {}",
                data_path.display()
            );
            fs::create_dir_all(data_path).await.map_err(|e| {
                OpenFanError::Config(format!(
                    "Failed to create controller data directory '{}': {}",
                    data_path.display(),
                    e
                ))
            })?;
        }

        // Verify we can write to it
        let test_file = data_path.join(".write_test");
        fs::write(&test_file, "test").await.map_err(|e| {
            OpenFanError::Config(format!(
                "Controller data directory '{}' is not writable: {}",
                data_path.display(),
                e
            ))
        })?;
        let _ = fs::remove_file(&test_file).await;

        Ok(())
    }

    /// Write TOML content atomically
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
    // Alias access and modification
    // =========================================================================

    async fn load_aliases(data_path: &Path) -> Result<AliasData> {
        let path = data_path.join("aliases.toml");

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

    /// Get read lock on alias data
    pub async fn aliases(&self) -> tokio::sync::RwLockReadGuard<'_, AliasData> {
        self.aliases.read().await
    }

    /// Get write lock on alias data
    pub async fn aliases_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, AliasData> {
        self.aliases.write().await
    }

    /// Save alias data to disk
    pub async fn save_aliases(&self) -> Result<()> {
        let aliases = self.aliases.read().await;
        let path = self.data_path.join("aliases.toml");

        let content = aliases
            .to_toml()
            .map_err(|e| OpenFanError::Config(format!("Failed to serialize aliases: {}", e)))?;

        Self::write_toml(&path, &content).await?;

        debug!(
            "Saved aliases for controller '{}' to {}",
            self.id,
            path.display()
        );
        Ok(())
    }

    // =========================================================================
    // Profile access and modification
    // =========================================================================

    async fn load_profiles(data_path: &Path) -> Result<ProfileData> {
        let path = data_path.join("profiles.toml");

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

    /// Get read lock on profile data
    pub async fn profiles(&self) -> tokio::sync::RwLockReadGuard<'_, ProfileData> {
        self.profiles.read().await
    }

    /// Get write lock on profile data
    pub async fn profiles_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, ProfileData> {
        self.profiles.write().await
    }

    /// Save profile data to disk
    pub async fn save_profiles(&self) -> Result<()> {
        let profiles = self.profiles.read().await;
        let path = self.data_path.join("profiles.toml");

        let content = profiles
            .to_toml()
            .map_err(|e| OpenFanError::Config(format!("Failed to serialize profiles: {}", e)))?;

        Self::write_toml(&path, &content).await?;

        debug!(
            "Saved profiles for controller '{}' to {}",
            self.id,
            path.display()
        );
        Ok(())
    }

    // =========================================================================
    // Thermal curve access and modification
    // =========================================================================

    async fn load_thermal_curves(data_path: &Path) -> Result<ThermalCurveData> {
        let path = data_path.join("thermal_curves.toml");

        if !path.exists() {
            debug!("Thermal curves file not found. Creating with defaults.");
            let data = ThermalCurveData::with_defaults();
            Self::write_toml(&path, &data.to_toml().unwrap()).await?;
            return Ok(data);
        }

        let content = fs::read_to_string(&path).await.map_err(|e| {
            OpenFanError::Config(format!("Failed to read thermal curves file: {}", e))
        })?;

        ThermalCurveData::from_toml(&content).map_err(|e| {
            OpenFanError::Config(format!("Failed to parse thermal curves file: {}", e))
        })
    }

    /// Get read lock on thermal curve data
    pub async fn thermal_curves(&self) -> tokio::sync::RwLockReadGuard<'_, ThermalCurveData> {
        self.thermal_curves.read().await
    }

    /// Get write lock on thermal curve data
    pub async fn thermal_curves_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, ThermalCurveData> {
        self.thermal_curves.write().await
    }

    /// Save thermal curve data to disk
    pub async fn save_thermal_curves(&self) -> Result<()> {
        let curves = self.thermal_curves.read().await;
        let path = self.data_path.join("thermal_curves.toml");

        let content = curves.to_toml().map_err(|e| {
            OpenFanError::Config(format!("Failed to serialize thermal curves: {}", e))
        })?;

        Self::write_toml(&path, &content).await?;

        debug!(
            "Saved thermal curves for controller '{}' to {}",
            self.id,
            path.display()
        );
        Ok(())
    }

    // =========================================================================
    // CFM mapping access and modification
    // =========================================================================

    async fn load_cfm_mappings(data_path: &Path) -> Result<CfmMappingData> {
        let path = data_path.join("cfm_mappings.toml");

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

    /// Get read lock on CFM mapping data
    pub async fn cfm_mappings(&self) -> tokio::sync::RwLockReadGuard<'_, CfmMappingData> {
        self.cfm_mappings.read().await
    }

    /// Get write lock on CFM mapping data
    pub async fn cfm_mappings_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, CfmMappingData> {
        self.cfm_mappings.write().await
    }

    /// Save CFM mapping data to disk
    pub async fn save_cfm_mappings(&self) -> Result<()> {
        let mappings = self.cfm_mappings.read().await;
        let path = self.data_path.join("cfm_mappings.toml");

        let content = mappings.to_toml().map_err(|e| {
            OpenFanError::Config(format!("Failed to serialize CFM mappings: {}", e))
        })?;

        Self::write_toml(&path, &content).await?;

        debug!(
            "Saved CFM mappings for controller '{}' to {}",
            self.id,
            path.display()
        );
        Ok(())
    }

    // =========================================================================
    // Board validation
    // =========================================================================

    /// Validate data against the controller's board
    ///
    /// Checks that profiles, aliases, and CFM mappings are compatible with the board's fan count.
    pub async fn validate(&self) -> Result<()> {
        let profiles = self.profiles.read().await;
        let aliases = self.aliases.read().await;
        let cfm_mappings = self.cfm_mappings.read().await;

        // Validate profiles
        for (name, profile) in &profiles.profiles {
            if profile.values.len() > self.board_info.fan_count {
                return Err(OpenFanError::Config(format!(
                    "Controller '{}': Profile '{}' has {} values but board '{}' only supports {} fans",
                    self.id,
                    name,
                    profile.values.len(),
                    self.board_info.name,
                    self.board_info.fan_count
                )));
            }
            if profile.values.len() < self.board_info.fan_count {
                warn!(
                    "Controller '{}': Profile '{}' has {} values but board has {} fans (will use defaults for extra fans)",
                    self.id, name, profile.values.len(), self.board_info.fan_count
                );
            }
        }

        // Validate aliases
        if let Some(&max_id) = aliases.aliases.keys().max() {
            if max_id >= self.board_info.fan_count as u8 {
                return Err(OpenFanError::Config(format!(
                    "Controller '{}': Alias exists for fan {} but board '{}' only has {} fans (max ID: {})",
                    self.id,
                    max_id,
                    self.board_info.name,
                    self.board_info.fan_count,
                    self.board_info.fan_count - 1
                )));
            }
        }

        // Validate CFM mappings
        if let Some(&max_port) = cfm_mappings.mappings.keys().max() {
            if max_port >= self.board_info.fan_count as u8 {
                return Err(OpenFanError::Config(format!(
                    "Controller '{}': CFM mapping exists for port {} but board '{}' only has {} fans (max ID: {})",
                    self.id,
                    max_port,
                    self.board_info.name,
                    self.board_info.fan_count,
                    self.board_info.fan_count - 1
                )));
            }
        }

        Ok(())
    }

    /// Fill missing defaults for the controller's board
    ///
    /// Ensures aliases exist for all fans on the board.
    pub async fn fill_defaults(&self) -> Result<()> {
        let mut modified = false;

        {
            let mut aliases = self.aliases.write().await;
            for i in 0..self.board_info.fan_count as u8 {
                if !aliases.aliases.contains_key(&i) {
                    debug!(
                        "Controller '{}': Adding missing alias for fan {}",
                        self.id, i
                    );
                    aliases.set(i, format!("Fan #{}", i + 1));
                    modified = true;
                }
            }
        }

        if modified {
            self.save_aliases().await?;
            info!(
                "Controller '{}': Added missing default aliases for {}",
                self.id, self.board_info.name
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfan_core::board::BoardType;
    use tempfile::TempDir;

    fn mock_board_info() -> BoardInfo {
        BoardType::OpenFanStandard.to_board_info()
    }

    #[tokio::test]
    async fn test_controller_data_load_creates_directory() {
        let temp_dir = TempDir::new().unwrap();
        let data = ControllerData::load("main", mock_board_info(), temp_dir.path())
            .await
            .unwrap();

        // Check directory was created
        assert!(temp_dir.path().join("controllers").join("main").exists());
        assert_eq!(data.id(), "main");
    }

    #[tokio::test]
    async fn test_controller_data_load_creates_default_files() {
        let temp_dir = TempDir::new().unwrap();
        let _ = ControllerData::load("main", mock_board_info(), temp_dir.path())
            .await
            .unwrap();

        let controller_dir = temp_dir.path().join("controllers").join("main");
        assert!(controller_dir.join("aliases.toml").exists());
        assert!(controller_dir.join("profiles.toml").exists());
        assert!(controller_dir.join("thermal_curves.toml").exists());
        assert!(controller_dir.join("cfm_mappings.toml").exists());
    }

    #[tokio::test]
    async fn test_controller_data_alias_operations() {
        let temp_dir = TempDir::new().unwrap();
        let data = ControllerData::load("main", mock_board_info(), temp_dir.path())
            .await
            .unwrap();

        // Set an alias
        {
            let mut aliases = data.aliases_mut().await;
            aliases.set(0, "CPU Fan".to_string());
        }

        // Save
        data.save_aliases().await.unwrap();

        // Reload and verify
        let data2 = ControllerData::load("main", mock_board_info(), temp_dir.path())
            .await
            .unwrap();
        let aliases = data2.aliases().await;
        assert_eq!(aliases.get(0), "CPU Fan");
    }

    #[tokio::test]
    async fn test_controller_data_profile_operations() {
        let temp_dir = TempDir::new().unwrap();
        let data = ControllerData::load("main", mock_board_info(), temp_dir.path())
            .await
            .unwrap();

        // Should have default profiles
        let profiles = data.profiles().await;
        assert!(profiles.contains("50% PWM"));
        assert!(profiles.contains("100% PWM"));
    }

    #[tokio::test]
    async fn test_controller_data_validate_valid() {
        let temp_dir = TempDir::new().unwrap();
        let data = ControllerData::load("main", mock_board_info(), temp_dir.path())
            .await
            .unwrap();

        // Valid aliases (within 10 fan limit)
        {
            let mut aliases = data.aliases_mut().await;
            aliases.set(0, "Fan 1".to_string());
            aliases.set(9, "Fan 10".to_string());
        }

        assert!(data.validate().await.is_ok());
    }

    #[tokio::test]
    async fn test_controller_data_validate_invalid_alias() {
        let temp_dir = TempDir::new().unwrap();
        let data = ControllerData::load("main", mock_board_info(), temp_dir.path())
            .await
            .unwrap();

        // Invalid alias (fan 10 doesn't exist on 10-fan board, max ID is 9)
        {
            let mut aliases = data.aliases_mut().await;
            aliases.set(10, "Invalid Fan".to_string());
        }

        let result = data.validate().await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Alias exists for fan 10"));
    }

    #[tokio::test]
    async fn test_controller_data_fill_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let data = ControllerData::load("main", mock_board_info(), temp_dir.path())
            .await
            .unwrap();

        // Clear aliases
        {
            let mut aliases = data.aliases_mut().await;
            aliases.aliases.clear();
        }
        data.save_aliases().await.unwrap();

        // Fill defaults
        data.fill_defaults().await.unwrap();

        // Should have 10 aliases for standard board
        let aliases = data.aliases().await;
        assert_eq!(aliases.aliases.len(), 10);
        assert_eq!(aliases.get(0), "Fan #1");
        assert_eq!(aliases.get(9), "Fan #10");
    }

    #[tokio::test]
    async fn test_multiple_controllers() {
        let temp_dir = TempDir::new().unwrap();

        // Create two controllers
        let main = ControllerData::load("main", mock_board_info(), temp_dir.path())
            .await
            .unwrap();
        let gpu = ControllerData::load("gpu", mock_board_info(), temp_dir.path())
            .await
            .unwrap();

        // Set different aliases
        {
            let mut main_aliases = main.aliases_mut().await;
            main_aliases.set(0, "Main CPU".to_string());
        }
        {
            let mut gpu_aliases = gpu.aliases_mut().await;
            gpu_aliases.set(0, "GPU Primary".to_string());
        }

        main.save_aliases().await.unwrap();
        gpu.save_aliases().await.unwrap();

        // Verify they are independent
        let main_aliases = main.aliases().await;
        let gpu_aliases = gpu.aliases().await;

        assert_eq!(main_aliases.get(0), "Main CPU");
        assert_eq!(gpu_aliases.get(0), "GPU Primary");

        // Verify separate directories
        assert!(temp_dir.path().join("controllers").join("main").exists());
        assert!(temp_dir.path().join("controllers").join("gpu").exists());
    }
}

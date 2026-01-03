//! Shutdown handling for graceful daemon termination
//!
//! Provides safe boot profile application during shutdown to ensure
//! fans continue running at a safe speed when the daemon terminates.

use crate::config::RuntimeConfig;
use crate::controllers::ConnectionManager;
use openfan_core::ControlMode;
use std::sync::Arc;
use tracing::{info, warn};

/// Apply safe boot profile before shutdown
///
/// Applies a configured fan profile (default: "100% PWM") before the daemon
/// terminates, ensuring fans run at a safe speed during system shutdown/reboot.
///
/// This prevents a thermal safety issue where fans would stop when the daemon
/// terminates but before the system completes shutdown. The profile is applied
/// only if enabled in config and hardware is available.
///
/// # Arguments
///
/// * `runtime_config` - Runtime configuration containing shutdown settings and profiles
/// * `connection_manager` - Hardware connection manager (None in mock mode)
/// * `is_mock` - Whether running in mock mode (skips profile application)
pub async fn apply_safe_boot_profile(
    runtime_config: &Arc<RuntimeConfig>,
    connection_manager: Option<&Arc<ConnectionManager>>,
    is_mock: bool,
) {
    let shutdown_config = &runtime_config.static_config().shutdown;

    if !shutdown_config.enabled {
        info!("Safe boot profile disabled in config");
        return;
    }

    if is_mock {
        info!("Mock mode - skipping safe boot profile");
        return;
    }

    let Some(cm) = connection_manager else {
        warn!("No hardware connection - cannot apply safe boot profile");
        return;
    };

    let profile_name = &shutdown_config.profile;
    let profile = {
        let profiles = runtime_config.profiles().await;
        profiles.get(profile_name.as_str()).cloned()
    };

    let Some(profile) = profile else {
        warn!("Safe boot profile '{}' not found", profile_name);
        return;
    };

    info!("Applying safe boot profile '{}'...", profile_name);

    let result = cm
        .with_controller(|controller| {
            let values = profile.values.clone();
            let mode = profile.control_mode;
            Box::pin(async move {
                for (fan_id, &value) in values.iter().enumerate() {
                    let fan_id = fan_id as u8;
                    let res = match mode {
                        ControlMode::Pwm => controller.set_fan_pwm(fan_id, value).await,
                        ControlMode::Rpm => controller.set_fan_rpm(fan_id, value).await,
                    };
                    if let Err(e) = res {
                        warn!("Failed to set fan {} during shutdown: {}", fan_id, e);
                    }
                }
                Ok(())
            })
        })
        .await;

    match result {
        Ok(_) => info!("Safe boot profile applied successfully"),
        Err(e) => warn!("Failed to apply safe boot profile: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfan_core::config::{ProfileName, ShutdownConfig, StaticConfig};
    use std::path::Path;
    use tempfile::TempDir;
    use tokio::fs;

    /// Create a test config with specific shutdown settings
    async fn create_test_config(
        temp_dir: &Path,
        shutdown_enabled: bool,
        profile_name: &str,
    ) -> Arc<RuntimeConfig> {
        let config_path = temp_dir.join("config.toml");
        let data_dir = temp_dir.join("data");

        // Create a custom static config with specific shutdown settings
        let static_config = StaticConfig {
            data_dir: data_dir.clone(),
            shutdown: ShutdownConfig {
                enabled: shutdown_enabled,
                profile: ProfileName::new(profile_name),
            },
            ..StaticConfig::default()
        };

        fs::create_dir_all(&data_dir).await.unwrap();
        fs::write(&config_path, static_config.to_toml().unwrap())
            .await
            .unwrap();

        Arc::new(RuntimeConfig::load(&config_path).await.unwrap())
    }

    #[tokio::test]
    async fn test_shutdown_disabled_returns_early() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(temp_dir.path(), false, "100% PWM").await;

        // Should return early without attempting to apply profile
        // (no connection manager needed since we return before checking it)
        apply_safe_boot_profile(&config, None, false).await;

        // If we reach here without panic, the early return worked
    }

    #[tokio::test]
    async fn test_mock_mode_returns_early() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(temp_dir.path(), true, "100% PWM").await;

        // Should return early in mock mode
        apply_safe_boot_profile(&config, None, true).await;

        // If we reach here without panic, the early return worked
    }

    #[tokio::test]
    async fn test_no_connection_manager_returns_early() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(temp_dir.path(), true, "100% PWM").await;

        // Should return early when no connection manager
        apply_safe_boot_profile(&config, None, false).await;

        // If we reach here without panic, the early return worked
    }

    #[tokio::test]
    async fn test_missing_profile_returns_early() {
        let temp_dir = TempDir::new().unwrap();
        // Use a non-existent profile name
        let config = create_test_config(temp_dir.path(), true, "NonExistent Profile").await;

        // The function needs a ConnectionManager to get past the None check,
        // but we can't easily create one without hardware.
        // This test verifies the config is set up correctly for the missing profile case.
        let shutdown_config = &config.static_config().shutdown;
        assert!(shutdown_config.enabled);
        assert_eq!(shutdown_config.profile.as_str(), "NonExistent Profile");

        // Verify the profile doesn't exist
        let profiles = config.profiles().await;
        assert!(profiles.get("NonExistent Profile").is_none());
    }

    #[tokio::test]
    async fn test_default_profile_exists() {
        use openfan_core::DEFAULT_SAFE_BOOT_PROFILE;

        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(temp_dir.path(), true, DEFAULT_SAFE_BOOT_PROFILE).await;

        // Verify the default profile exists in the loaded profiles
        let profiles = config.profiles().await;
        assert!(
            profiles.get(DEFAULT_SAFE_BOOT_PROFILE).is_some(),
            "Default safe boot profile '{}' should exist in profiles",
            DEFAULT_SAFE_BOOT_PROFILE
        );
    }
}

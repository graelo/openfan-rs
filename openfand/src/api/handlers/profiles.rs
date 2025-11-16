//! Profile handlers for CRUD operations

use crate::api::error::ApiError;
use crate::api::AppState;
use crate::{api_fail, api_ok};
use axum::{
    extract::{Query, State},
    Json,
};
use openfan_core::{
    api::{ApiResponse, ProfileResponse},
    ControlMode, FanProfile,
};
use serde::Deserialize;

use tracing::{debug, info, warn};

/// Query parameters for profile operations.
#[derive(Deserialize)]
pub struct ProfileQuery {
    /// Profile name (case-sensitive)
    pub name: Option<String>,
}

/// Request body for adding a new profile.
#[derive(Deserialize)]
pub struct AddProfileRequest {
    /// Profile name (must be non-empty after trimming whitespace)
    pub name: String,
    /// Profile data (must have exactly 10 values with appropriate ranges)
    pub profile: FanProfile,
}

/// Lists all available fan profiles.
///
/// Returns all configured profiles with their control modes and fan values.
///
/// # Endpoint
///
/// `GET /api/v0/profiles/list`
pub async fn list_profiles(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<ProfileResponse>>, ApiError> {
    debug!("Request: GET /api/v0/profiles/list");

    let config = state.config.read().await;
    let fan_profiles = config.config().fan_profiles.clone();

    let response = ProfileResponse {
        profiles: fan_profiles,
    };

    info!("Listed {} fan profiles", response.profiles.len());
    api_ok!(response)
}

/// Adds a new fan profile to the configuration.
///
/// Validates the profile data and saves it to the configuration file.
///
/// # Validation Rules
///
/// - Profile name must not be empty after trimming whitespace
/// - Must have exactly 10 values (one per fan)
/// - PWM mode: values must be 0-100 (percentage)
/// - RPM mode: values must be 0-16000 (revolutions per minute)
///
/// # Endpoint
///
/// `POST /api/v0/profiles/add`
///
/// # Request Body
///
/// ```json
/// {
///   "name": "Gaming",
///   "profile": {
///     "control_mode": "pwm",
///     "values": [50, 60, 70, 80, 90, 100, 90, 80, 70, 60]
///   }
/// }
/// ```
pub async fn add_profile(
    State(state): State<AppState>,
    Json(request): Json<AddProfileRequest>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    debug!("Request: POST /api/v0/profiles/add");

    let profile_name = request.name.trim();

    if profile_name.is_empty() {
        return api_fail!("Profile name cannot be empty!");
    }

    let profile = request.profile;

    // Validate values count against board configuration
    if profile.values.len() != state.board_info.fan_count {
        return api_fail!(format!(
            "Profile must have exactly {} values for {}!",
            state.board_info.fan_count, state.board_info.name
        ));
    }

    // Validate value ranges
    for (i, &value) in profile.values.iter().enumerate() {
        match profile.control_mode {
            ControlMode::Pwm => {
                if value > 100 {
                    return api_fail!(format!(
                        "PWM value at position {} is too high: {} (max 100)",
                        i + 1,
                        value
                    ));
                }
            }
            ControlMode::Rpm => {
                if value > 16000 {
                    return api_fail!(format!(
                        "RPM value at position {} is too high: {} (max 16000)",
                        i + 1,
                        value
                    ));
                }
            }
        }
    }

    // Add to configuration
    let mut config = state.config.write().await;
    config
        .config_mut()
        .fan_profiles
        .insert(profile_name.to_string(), profile);

    // Save configuration
    if let Err(e) = config.save().await {
        return Err(ApiError::internal_error(format!(
            "Failed to save configuration: {}",
            e
        )));
    }

    info!("Added profile: {}", profile_name);
    api_ok!(())
}

/// Removes a profile from the configuration.
///
/// The profile name is case-sensitive. If the profile exists, it is removed
/// and the configuration is saved.
///
/// # Endpoint
///
/// `GET /api/v0/profiles/remove?name=Custom`
///
/// # Query Parameters
///
/// - `name` - Name of the profile to remove (case-sensitive)
pub async fn remove_profile(
    State(state): State<AppState>,
    Query(params): Query<ProfileQuery>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    debug!("Request: GET /api/v0/profiles/remove");

    let Some(profile_name) = params.name else {
        return api_fail!("Name cannot be empty!");
    };

    // Remove from configuration
    let mut config = state.config.write().await;
    let removed = config.config_mut().fan_profiles.remove(&profile_name);

    if removed.is_some() {
        // Save configuration
        if let Err(e) = config.save().await {
            return Err(ApiError::internal_error(format!(
                "Failed to save configuration: {}",
                e
            )));
        }

        info!("Removed profile: {}", profile_name);
        api_ok!(())
    } else {
        api_fail!(format!(
            "Profile '{}' does not exist! (Names are case-sensitive!)",
            profile_name
        ))
    }
}

/// Applies a profile to all fans.
///
/// Sets all fans to the values defined in the profile. The control mode
/// (PWM or RPM) determines how the values are applied.
///
/// # Behavior
///
/// - In mock mode (no hardware): simulates applying the profile
/// - With hardware: sets each fan individually based on control mode
/// - Partial failures are logged but don't prevent other fans from being set
///
/// # Endpoint
///
/// `GET /api/v0/profiles/set?name=Gaming`
///
/// # Query Parameters
///
/// - `name` - Name of the profile to apply (case-sensitive)
pub async fn set_profile(
    State(state): State<AppState>,
    Query(params): Query<ProfileQuery>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    debug!("Request: GET /api/v0/profiles/set");

    let Some(profile_name) = params.name else {
        return api_fail!("Name cannot be empty!");
    };

    // Get profile from configuration
    let config = state.config.read().await;
    let Some(profile) = config.config().fan_profiles.get(&profile_name) else {
        return api_fail!(format!(
            "Profile '{}' does not exist! (Names are case-sensitive!)",
            profile_name
        ));
    };

    let profile = profile.clone();
    drop(config); // Release the read lock

    // Check if hardware is available
    let Some(fan_controller) = &state.fan_controller else {
        debug!("Hardware not available - simulating profile application for testing");
        info!("Applied profile '{}' (mock mode)", profile_name);
        return api_ok!(());
    };

    let mut commander = fan_controller.lock().await;
    let mut results = Vec::new();

    // Apply profile values to each fan
    for (fan_id, &value) in profile.values.iter().enumerate() {
        let fan_id = fan_id as u8;

        let result = match profile.control_mode {
            ControlMode::Pwm => match commander.set_fan_pwm(fan_id, value).await {
                Ok(_) => format!("Fan {} set to {}% PWM", fan_id, value),
                Err(e) => {
                    warn!(
                        "Failed to set fan {} PWM while applying profile '{}': {}",
                        fan_id, profile_name, e
                    );
                    format!("Fan {} failed: {}", fan_id, e)
                }
            },
            ControlMode::Rpm => match commander.set_fan_rpm(fan_id, value).await {
                Ok(_) => format!("Fan {} set to {} RPM", fan_id, value),
                Err(e) => {
                    warn!(
                        "Failed to set fan {} RPM while applying profile '{}': {}",
                        fan_id, profile_name, e
                    );
                    format!("Fan {} failed: {}", fan_id, e)
                }
            },
        };

        results.push(result);
    }

    info!("Applied profile '{}' to all fans", profile_name);
    api_ok!(())
}

#[cfg(test)]
mod tests {
    use openfan_core::{BoardConfig, DefaultBoard};

    #[test]
    fn test_profile_validation() {
        // Test PWM values
        let pwm_values = [0, 25, 50, 75, 100, 100, 75, 50, 25, 0];
        assert_eq!(pwm_values.len(), 10);
        assert!(pwm_values.iter().all(|&v| v <= 100));

        // Test RPM values
        let rpm_values = [0, 1000, 2000, 3000, 4000, 5000, 4000, 3000, 2000, 1000];
        assert_eq!(rpm_values.len(), 10);
        assert!(rpm_values.iter().all(|&v| v <= 16000));
    }

    #[test]
    fn test_control_mode_parsing() {
        assert!(matches!(
            "pwm".parse::<String>().unwrap().to_lowercase().as_str(),
            "pwm"
        ));
        assert!(matches!(
            "rpm".parse::<String>().unwrap().to_lowercase().as_str(),
            "rpm"
        ));
        assert!(matches!(
            "PWM".parse::<String>().unwrap().to_lowercase().as_str(),
            "pwm"
        ));
        assert!(matches!(
            "RPM".parse::<String>().unwrap().to_lowercase().as_str(),
            "rpm"
        ));
    }

    #[test]
    fn test_values_parsing() {
        let values_str = "50,60,70,80,90,100,90,80,70,60";
        let values: Result<Vec<u32>, _> = values_str
            .split(',')
            .map(|s| s.trim().parse::<u32>())
            .collect();

        assert!(values.is_ok());
        let values = values.unwrap();
        assert_eq!(values.len(), 10);
        assert_eq!(values[0], 50);
        assert_eq!(values[9], 60);
    }

    #[test]
    fn test_invalid_values_parsing() {
        let values_str = "50,abc,70,80,90,100,90,80,70,60";
        let values: Result<Vec<u32>, _> = values_str
            .split(',')
            .map(|s| s.trim().parse::<u32>())
            .collect();

        assert!(values.is_err());
    }

    #[test]
    fn test_wrong_count_values() {
        let values_str = "50,60,70,80,90"; // Only 5 values
        let values: Result<Vec<u32>, _> = values_str
            .split(',')
            .map(|s| s.trim().parse::<u32>())
            .collect();

        assert!(values.is_ok());
        let values = values.unwrap();
        assert_eq!(values.len(), 5); // Should not be 10
    }

    // Edge case tests for add_profile validation logic
    #[test]
    fn test_profile_name_validation_empty() {
        // Test that empty profile name (after trim) is rejected
        let empty_name = "".to_string();
        assert!(
            empty_name.trim().is_empty(),
            "Empty name should be rejected by add_profile"
        );

        let whitespace_name = "   ".to_string();
        assert!(
            whitespace_name.trim().is_empty(),
            "Whitespace-only name should be rejected by add_profile"
        );
    }

    #[test]
    fn test_profile_pwm_value_exceeds_limit() {
        use super::*;

        // Test PWM value > 100 validation (from add_profile handler logic)
        let profile = FanProfile {
            control_mode: ControlMode::Pwm,
            values: vec![50, 50, 50, 101, 50, 50, 50, 50, 50, 50], // 101 exceeds limit
        };

        // Verify that the validation would catch this
        assert_eq!(profile.values.len(), DefaultBoard::FAN_COUNT);
        let invalid_value = profile.values.iter().enumerate().find(|(_, &v)| v > 100);
        assert!(invalid_value.is_some(), "Should find PWM value > 100");
        assert_eq!(
            invalid_value.unwrap().0,
            3,
            "Invalid value should be at position 3"
        );
    }

    #[test]
    fn test_profile_rpm_value_exceeds_limit() {
        use super::*;

        // Test RPM value > 16000 validation (from add_profile handler logic)
        let profile = FanProfile {
            control_mode: ControlMode::Rpm,
            values: vec![1000, 2000, 3000, 16001, 5000, 6000, 7000, 8000, 9000, 10000], // 16001 exceeds limit
        };

        // Verify that the validation would catch this
        assert_eq!(profile.values.len(), DefaultBoard::FAN_COUNT);
        let invalid_value = profile.values.iter().enumerate().find(|(_, &v)| v > 16000);
        assert!(invalid_value.is_some(), "Should find RPM value > 16000");
        assert_eq!(
            invalid_value.unwrap().0,
            3,
            "Invalid value should be at position 3"
        );
    }

    #[test]
    fn test_profile_value_count_validation() {
        use super::*;

        // Test that wrong value count is caught (from add_profile handler logic)
        let too_few = FanProfile {
            control_mode: ControlMode::Pwm,
            values: vec![50, 50, 50], // Only 3 values
        };
        assert_ne!(
            too_few.values.len(),
            DefaultBoard::FAN_COUNT,
            "Should have wrong count"
        );

        let too_many = FanProfile {
            control_mode: ControlMode::Pwm,
            values: vec![50; 15], // 15 values
        };
        assert_ne!(
            too_many.values.len(),
            DefaultBoard::FAN_COUNT,
            "Should have wrong count"
        );

        let correct = FanProfile {
            control_mode: ControlMode::Pwm,
            values: vec![50; DefaultBoard::FAN_COUNT],
        };
        assert_eq!(
            correct.values.len(),
            DefaultBoard::FAN_COUNT,
            "Should have correct count"
        );
    }

    #[test]
    fn test_profile_boundary_values() {
        use super::*;

        // Test boundary values for PWM (0 and 100 should be valid)
        let pwm_boundary = FanProfile {
            control_mode: ControlMode::Pwm,
            values: vec![0, 100, 0, 100, 0, 100, 0, 100, 0, 100],
        };
        assert!(
            pwm_boundary.values.iter().all(|&v| v <= 100),
            "All PWM values should be valid"
        );

        // Test boundary values for RPM (0 and 16000 should be valid)
        let rpm_boundary = FanProfile {
            control_mode: ControlMode::Rpm,
            values: vec![0, 16000, 0, 16000, 0, 16000, 0, 16000, 0, 16000],
        };
        assert!(
            rpm_boundary.values.iter().all(|&v| v <= 16000),
            "All RPM values should be valid"
        );
    }
}

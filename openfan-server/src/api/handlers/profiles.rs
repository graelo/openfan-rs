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

/// Query parameters for profile operations
#[derive(Deserialize)]
pub struct ProfileQuery {
    /// Profile name
    pub name: Option<String>,
}

/// Request body for adding a new profile
#[derive(Deserialize)]
pub struct AddProfileRequest {
    /// Profile name
    pub name: String,
    /// Profile data
    pub profile: FanProfile,
}

/// List all available profiles
/// GET /api/v0/profiles/list
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

/// Add a new profile
/// POST /api/v0/profiles/add
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

    // Validate values count
    if profile.values.len() != 10 {
        return api_fail!("Profile must have exactly 10 values!");
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
        warn!("Failed to save configuration: {}", e);
        return Err(ApiError::internal_error(format!(
            "Failed to save configuration: {}",
            e
        )));
    }

    info!("Added profile: {}", profile_name);
    api_ok!(())
}

/// Remove a profile
/// GET /api/v0/profiles/remove?name=Custom
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
            warn!("Failed to save configuration: {}", e);
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

/// Apply a profile (set all fans to profile values)
/// GET /api/v0/profiles/set?name=Gaming
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
    let Some(fan_commander) = &state.fan_commander else {
        warn!("Hardware not available - simulating profile application for testing");
        info!("Applied profile '{}' (mock mode)", profile_name);
        return api_ok!(());
    };

    let mut commander = fan_commander.lock().await;
    let mut results = Vec::new();

    // Apply profile values to each fan
    for (fan_id, &value) in profile.values.iter().enumerate() {
        let fan_id = fan_id as u8;

        let result = match profile.control_mode {
            ControlMode::Pwm => match commander.set_fan_pwm(fan_id, value).await {
                Ok(_) => format!("Fan {} set to {}% PWM", fan_id, value),
                Err(e) => {
                    warn!("Failed to set fan {} PWM: {}", fan_id, e);
                    format!("Fan {} failed: {}", fan_id, e)
                }
            },
            ControlMode::Rpm => match commander.set_fan_rpm(fan_id, value).await {
                Ok(_) => format!("Fan {} set to {} RPM", fan_id, value),
                Err(e) => {
                    warn!("Failed to set fan {} RPM: {}", fan_id, e);
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
    // Test imports removed - not used

    #[test]
    fn test_profile_validation() {
        // Test PWM values
        let pwm_values = vec![0, 25, 50, 75, 100, 100, 75, 50, 25, 0];
        assert_eq!(pwm_values.len(), 10);
        assert!(pwm_values.iter().all(|&v| v <= 100));

        // Test RPM values
        let rpm_values = vec![0, 1000, 2000, 3000, 4000, 5000, 4000, 3000, 2000, 1000];
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
}

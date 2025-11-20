//! Fan handlers for status and control endpoints

use crate::api::error::ApiError;
use crate::api::AppState;
use crate::{api_fail, api_ok};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use openfan_core::api::{ApiResponse, FanStatusResponse};
use serde::Deserialize;
use std::collections::HashMap;

use tracing::debug;

/// Query parameters for fan control endpoints.
#[derive(Deserialize)]
pub struct FanControlQuery {
    /// Value to set - PWM percentage (0-100) or RPM (0-16000)
    pub value: Option<f64>,
}

/// Retrieves the current status of all fans.
///
/// Returns RPM readings and cached PWM values for all fans in the system.
///
/// # Behavior
///
/// - With hardware: Returns actual RPM readings and cached PWM values
/// - Mock mode: Returns simulated fan data for testing
///
/// # Note
///
/// PWM values are cached because the hardware does not support reading them back.
///
/// # Endpoint
///
/// `GET /api/v0/fan/status`
pub async fn get_status(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<FanStatusResponse>>, ApiError> {
    debug!("Request: GET /api/v0/fan/status");

    // Check if hardware is available
    let Some(fan_controller) = &state.fan_controller else {
        debug!("Hardware not available - returning mock fan status data");
        // Return mock fan data for testing/development
        let mut mock_rpms = HashMap::new();
        let mut mock_pwms = HashMap::new();
        for i in 0..state.board_info.fan_count as u8 {
            mock_rpms.insert(i, 1500 + (i as u32 * 100)); // Mock RPM values
            mock_pwms.insert(i, 50 + (i as u32 * 5)); // Mock PWM values
        }
        let mock_status = FanStatusResponse {
            rpms: mock_rpms,
            pwms: mock_pwms,
        };
        return api_ok!(mock_status);
    };

    // Get RPM data from hardware
    let mut controller = fan_controller.lock().await;
    match controller.get_all_fan_rpm().await {
        Ok(rpm_map) => {
            // Get cached PWM data
            // Note: Hardware does not support reading PWM values, so we return cached values
            let pwm_map = controller.get_all_fan_pwm();
            debug!(
                "Fan status retrieved - RPM: {:?}, PWM: {:?}",
                rpm_map, pwm_map
            );
            let status = FanStatusResponse {
                rpms: rpm_map,
                pwms: pwm_map,
            };
            api_ok!(status)
        }
        Err(e) => Err(ApiError::from(e)),
    }
}

/// Sets the PWM value for all fans simultaneously.
///
/// # Validation
///
/// - PWM values are automatically clamped to 0-100 range
/// - Values outside this range are adjusted without error
///
/// # Endpoint
///
/// `GET /api/v0/fan/all/set?value=50`
///
/// # Query Parameters
///
/// - `value` - PWM percentage (0-100, automatically clamped)
pub async fn set_all_fans(
    State(state): State<AppState>,
    Query(params): Query<FanControlQuery>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    debug!("Request: GET /api/v0/fan/all/set");

    let Some(value) = params.value else {
        return api_fail!("Missing 'value' parameter");
    };

    // Validate and clamp PWM value
    let pwm_value = value.clamp(0.0, 100.0) as u32;

    debug!("Setting all fans to {}% PWM", pwm_value);

    // Check if hardware is available
    let Some(fan_controller) = &state.fan_controller else {
        debug!("Hardware not available - simulating fan PWM set for testing");
        return api_ok!(());
    };

    // Send command to hardware
    let mut controller = fan_controller.lock().await;
    match controller.set_all_fan_pwm(pwm_value).await {
        Ok(response) => {
            debug!("Set all fans response: {}", response);
            api_ok!(())
        }
        Err(e) => Err(ApiError::from(e)),
    }
}

/// Sets the PWM value for a specific fan.
///
/// # Validation
///
/// - Fan ID must be 0-9 (corresponding to 10 fans)
/// - PWM values are automatically clamped to 0-100 range
///
/// # Endpoints
///
/// - `GET /api/v0/fan/:id/pwm?value=50` (current)
/// - `GET /api/v0/fan/:id/set?value=50` (legacy)
///
/// # Path Parameters
///
/// - `id` - Fan identifier (0-9)
///
/// # Query Parameters
///
/// - `value` - PWM percentage (0-100, automatically clamped)
pub async fn set_fan_pwm(
    State(state): State<AppState>,
    Path(fan_id): Path<String>,
    Query(params): Query<FanControlQuery>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    debug!("Request: GET /api/v0/fan/{}/pwm", fan_id);

    // Parse and validate fan ID
    let fan_index = fan_id
        .parse::<u8>()
        .map_err(|_| ApiError::bad_request(format!("Invalid fan ID: {}", fan_id)))?;

    // Validate fan ID against board configuration
    state.board_info.validate_fan_id(fan_index)?;

    let Some(value) = params.value else {
        return api_fail!("Missing 'value' parameter");
    };

    // Validate and clamp PWM value
    let pwm_value = value.clamp(0.0, 100.0) as u32;

    debug!("Setting fan {} to {}% PWM", fan_index, pwm_value);

    // Check if hardware is available
    let Some(fan_controller) = &state.fan_controller else {
        debug!(
            "Hardware not available - simulating fan {} PWM set for testing",
            fan_index
        );
        return api_ok!(());
    };

    // Send command to hardware
    let mut controller = fan_controller.lock().await;
    match controller.set_fan_pwm(fan_index, pwm_value).await {
        Ok(response) => {
            debug!("Set fan {} PWM response: {}", fan_index, response);
            api_ok!(())
        }
        Err(e) => Err(ApiError::service_unavailable(format!(
            "Failed to set fan {} PWM: {}",
            fan_index, e
        ))),
    }
}

/// Retrieves the current RPM reading for a specific fan.
///
/// # Validation
///
/// - Fan ID must be 0-9 (corresponding to 10 fans)
///
/// # Behavior
///
/// - With hardware: Returns actual RPM reading from the fan
/// - Mock mode: Returns simulated RPM value
///
/// # Endpoint
///
/// `GET /api/v0/fan/:id/rpm/get`
///
/// # Path Parameters
///
/// - `id` - Fan identifier (0-9)
pub async fn get_fan_rpm(
    Path(fan_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<u32>>, ApiError> {
    debug!("Request: GET /api/v0/fan/{}/rpm/get", fan_id);

    // Parse and validate fan ID
    let fan_index = fan_id
        .parse::<u8>()
        .map_err(|_| ApiError::bad_request(format!("Invalid fan ID: {}", fan_id)))?;

    // Validate fan ID against board configuration
    state.board_info.validate_fan_id(fan_index)?;

    // Check if hardware is available
    let Some(fan_controller) = &state.fan_controller else {
        debug!(
            "Hardware not available - returning mock RPM for fan {}",
            fan_index
        );
        let mock_rpm = 1500 + (fan_index as u32 * 100); // Mock RPM value
        return api_ok!(mock_rpm);
    };

    // Get single fan RPM from hardware
    let mut controller = fan_controller.lock().await;
    match controller.get_single_fan_rpm(fan_index).await {
        Ok(rpm) => {
            debug!("Fan {} RPM: {}", fan_index, rpm);
            api_ok!(rpm)
        }
        Err(e) => Err(ApiError::service_unavailable(format!(
            "Failed to get fan {} RPM: {}",
            fan_index, e
        ))),
    }
}

/// Sets the target RPM for a specific fan.
///
/// # Validation
///
/// - Fan ID must be 0-9 (corresponding to 10 fans)
/// - RPM values are clamped: values > 16000 become 16000, values < 480 become 0
/// - Minimum operational RPM is 480 (below this, fan is set to 0/off)
///
/// # Endpoint
///
/// `GET /api/v0/fan/:id/rpm?value=1000`
///
/// # Path Parameters
///
/// - `id` - Fan identifier (0-9)
///
/// # Query Parameters
///
/// - `value` - Target RPM (0-16000, with automatic clamping)
pub async fn set_fan_rpm(
    State(state): State<AppState>,
    Path(fan_id): Path<String>,
    Query(params): Query<FanControlQuery>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    debug!("Request: GET /api/v0/fan/{}/rpm", fan_id);

    // Parse and validate fan ID
    let fan_index = fan_id
        .parse::<u8>()
        .map_err(|_| ApiError::bad_request(format!("Invalid fan ID: {}", fan_id)))?;

    // Validate fan ID against board configuration
    state.board_info.validate_fan_id(fan_index)?;

    let Some(value) = params.value else {
        return api_fail!("Missing 'value' parameter");
    };

    // Validate and clamp RPM value
    let rpm_value = if value > 16000.0 {
        16000.0
    } else if value < 480.0 {
        0.0 // Set to 0 if below minimum
    } else {
        value
    } as u32;

    debug!("Setting fan {} to {} RPM", fan_index, rpm_value);

    // Check if hardware is available
    let Some(fan_controller) = &state.fan_controller else {
        debug!(
            "Hardware not available - simulating fan {} RPM set for testing",
            fan_index
        );
        return api_ok!(());
    };

    // Send command to hardware
    let mut controller = fan_controller.lock().await;
    match controller.set_fan_rpm(fan_index, rpm_value).await {
        Ok(response) => {
            debug!("Set fan {} RPM response: {}", fan_index, response);
            api_ok!(())
        }
        Err(e) => Err(ApiError::service_unavailable(format!(
            "Failed to set fan {} RPM: {}",
            fan_index, e
        ))),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_fan_id_parsing() {
        assert_eq!("0".parse::<u8>().unwrap(), 0);
        assert_eq!("9".parse::<u8>().unwrap(), 9);
        assert!("10".parse::<u8>().unwrap() > 9);
        assert!("abc".parse::<u8>().is_err());
    }
}

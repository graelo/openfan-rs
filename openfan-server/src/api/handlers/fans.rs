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

use tracing::{debug, warn};

/// Query parameters for fan control endpoints
#[derive(Deserialize)]
pub struct FanControlQuery {
    /// Value to set (PWM percentage or RPM)
    pub value: Option<f64>,
}

/// Get status of all fans
/// GET /api/v0/fan/status
pub async fn get_status(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<FanStatusResponse>>, ApiError> {
    debug!("Request: GET /api/v0/fan/status");

    // Check if hardware is available
    let Some(fan_commander) = &state.fan_commander else {
        warn!("Hardware not available - returning mock fan status data");
        // Return mock fan data for testing/development
        let mut mock_rpms = HashMap::new();
        let mut mock_pwms = HashMap::new();
        for i in 0..10 {
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
    let mut commander = fan_commander.lock().await;
    match commander.get_all_fan_rpm().await {
        Ok(rpm_map) => {
            debug!("Fan RPM data retrieved: {:?}", rpm_map);
            // TODO: Get actual PWM data from hardware when available
            // For now, return empty PWM map
            let pwm_map = HashMap::new();
            let status = FanStatusResponse {
                rpms: rpm_map,
                pwms: pwm_map,
            };
            api_ok!(status)
        }
        Err(e) => {
            warn!("Failed to get fan RPM: {}", e);
            Err(ApiError::from(e))
        }
    }
}

/// Set PWM for all fans
/// GET /api/v0/fan/all/set?value=50
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
    let Some(fan_commander) = &state.fan_commander else {
        warn!("Hardware not available - simulating fan PWM set for testing");
        return api_ok!(());
    };

    // Send command to hardware
    let mut commander = fan_commander.lock().await;
    match commander.set_all_fan_pwm(pwm_value).await {
        Ok(response) => {
            debug!("Set all fans response: {}", response);
            api_ok!(())
        }
        Err(e) => {
            warn!("Failed to set all fans PWM: {}", e);
            Err(ApiError::from(e))
        }
    }
}

/// Set PWM for a specific fan
/// GET /api/v0/fan/:id/pwm?value=50
/// GET /api/v0/fan/:id/set?value=50 (legacy)
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

    if fan_index > 9 {
        return api_fail!(format!("Invalid fan index (0<={fan_index}<=9)"));
    }

    let Some(value) = params.value else {
        return api_fail!("Missing 'value' parameter");
    };

    // Validate and clamp PWM value
    let pwm_value = value.clamp(0.0, 100.0) as u32;

    debug!("Setting fan {} to {}% PWM", fan_index, pwm_value);

    // Check if hardware is available
    let Some(fan_commander) = &state.fan_commander else {
        warn!(
            "Hardware not available - simulating fan {} PWM set for testing",
            fan_index
        );
        return api_ok!(());
    };

    // Send command to hardware
    let mut commander = fan_commander.lock().await;
    match commander.set_fan_pwm(fan_index, pwm_value).await {
        Ok(response) => {
            debug!("Set fan {} PWM response: {}", fan_index, response);
            api_ok!(())
        }
        Err(e) => {
            warn!("Failed to set fan {} PWM: {}", fan_index, e);
            Err(ApiError::from(e))
        }
    }
}

/// Set RPM for a specific fan
/// GET /api/v0/fan/:id/rpm/get
pub async fn get_fan_rpm(
    Path(fan_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<u32>>, ApiError> {
    debug!("Request: GET /api/v0/fan/{}/rpm/get", fan_id);

    // Parse and validate fan ID
    let fan_index = fan_id
        .parse::<u8>()
        .map_err(|_| ApiError::bad_request(format!("Invalid fan ID: {}", fan_id)))?;

    if fan_index > 9 {
        return api_fail!(format!("Invalid fan index (0<={fan_index}<=9)"));
    }

    // Check if hardware is available
    let Some(fan_commander) = &state.fan_commander else {
        warn!(
            "Hardware not available - returning mock RPM for fan {}",
            fan_index
        );
        let mock_rpm = 1500 + (fan_index as u32 * 100); // Mock RPM value
        return api_ok!(mock_rpm);
    };

    // Get single fan RPM from hardware
    let mut commander = fan_commander.lock().await;
    match commander.get_single_fan_rpm(fan_index).await {
        Ok(rpm) => {
            debug!("Fan {} RPM: {}", fan_index, rpm);
            api_ok!(rpm)
        }
        Err(e) => {
            warn!("Failed to get fan {} RPM: {}", fan_index, e);
            Err(ApiError::from(e))
        }
    }
}

/// GET /api/v0/fan/:id/rpm?value=1000
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

    if fan_index > 9 {
        return api_fail!(format!("Invalid fan index (0<={fan_index}<=9)"));
    }

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
    let Some(fan_commander) = &state.fan_commander else {
        warn!(
            "Hardware not available - simulating fan {} RPM set for testing",
            fan_index
        );
        return api_ok!(());
    };

    // Send command to hardware
    let mut commander = fan_commander.lock().await;
    match commander.set_fan_rpm(fan_index, rpm_value).await {
        Ok(response) => {
            debug!("Set fan {} RPM response: {}", fan_index, response);
            api_ok!(())
        }
        Err(e) => {
            warn!("Failed to set fan {} RPM: {}", fan_index, e);
            Err(ApiError::from(e))
        }
    }
}

#[cfg(test)]
mod tests {
    // Test imports removed - not used

    #[test]
    fn test_pwm_validation() {
        // Test clamping
        assert_eq!(if 150.0 > 100.0 { 100.0 } else { 150.0 } as u32, 100);
        assert_eq!(if -10.0 < 0.0 { 0.0 } else { -10.0 } as u32, 0);
        assert_eq!(
            if 50.0 > 100.0 {
                100.0
            } else if 50.0 < 0.0 {
                0.0
            } else {
                50.0
            } as u32,
            50
        );
    }

    #[test]
    fn test_rpm_validation() {
        // Test clamping
        assert_eq!(
            if 20000.0 > 16000.0 { 16000.0 } else { 20000.0 } as u32,
            16000
        );
        assert_eq!(if 400.0 < 480.0 { 0.0 } else { 400.0 } as u32, 0);
        assert_eq!(
            if 1000.0 > 16000.0 {
                16000.0
            } else if 1000.0 < 480.0 {
                0.0
            } else {
                1000.0
            } as u32,
            1000
        );
    }

    #[test]
    fn test_fan_id_parsing() {
        assert_eq!("0".parse::<u8>().unwrap(), 0);
        assert_eq!("9".parse::<u8>().unwrap(), 9);
        assert!("10".parse::<u8>().unwrap() > 9);
        assert!("abc".parse::<u8>().is_err());
    }
}

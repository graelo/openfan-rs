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
pub(crate) struct FanControlQuery {
    /// Value to set - PWM percentage (0-100) or target RPM (500-9000 per OpenFAN docs)
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
pub(crate) async fn get_status(
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
pub(crate) async fn set_all_fans(
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
pub(crate) async fn set_fan_pwm(
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
pub(crate) async fn get_fan_rpm(
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
/// - Fan ID must be valid for the board (0-9 for standard)
/// - RPM values must be within the board's target range (500-9000 per OpenFAN docs)
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
/// - `value` - Target RPM (must be within board's min/max target RPM range)
pub(crate) async fn set_fan_rpm(
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

    // Convert to u32 and validate against board's target RPM range
    let rpm_value = value as u32;
    state.board_info.validate_target_rpm(rpm_value)?;

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
    use super::*;

    #[test]
    fn test_target_rpm_validation_valid_range() {
        use openfan_core::BoardType;

        let board_info = BoardType::OpenFanStandard.to_board_info();

        // Values within 500-9000 should be valid (per OpenFAN docs)
        let valid_values = [500u32, 1000, 5000, 9000];

        for rpm in valid_values {
            assert!(
                board_info.validate_target_rpm(rpm).is_ok(),
                "RPM {} should be valid",
                rpm
            );
        }
    }

    #[test]
    fn test_target_rpm_validation_below_minimum() {
        use openfan_core::BoardType;

        let board_info = BoardType::OpenFanStandard.to_board_info();

        // Values below 500 should be invalid
        let invalid_values = [0u32, 100, 499];

        for rpm in invalid_values {
            assert!(
                board_info.validate_target_rpm(rpm).is_err(),
                "RPM {} should be invalid (below minimum)",
                rpm
            );
        }
    }

    #[test]
    fn test_target_rpm_validation_above_maximum() {
        use openfan_core::BoardType;

        let board_info = BoardType::OpenFanStandard.to_board_info();

        // Values above 9000 should be invalid
        let invalid_values = [9001u32, 10000, 16000];

        for rpm in invalid_values {
            assert!(
                board_info.validate_target_rpm(rpm).is_err(),
                "RPM {} should be invalid (above maximum)",
                rpm
            );
        }
    }

    #[test]
    fn test_target_rpm_boundary_values() {
        use openfan_core::BoardType;

        let board_info = BoardType::OpenFanStandard.to_board_info();

        // Exactly at boundaries should be valid
        assert!(board_info.validate_target_rpm(500).is_ok());
        assert!(board_info.validate_target_rpm(9000).is_ok());

        // Just outside boundaries should be invalid
        assert!(board_info.validate_target_rpm(499).is_err());
        assert!(board_info.validate_target_rpm(9001).is_err());
    }

    #[test]
    fn test_fan_control_query_deserialization() {
        // Test that FanControlQuery can be deserialized from query params
        let json_with_value = r#"{"value": 50.0}"#;
        let query: FanControlQuery = serde_json::from_str(json_with_value).unwrap();
        assert_eq!(query.value, Some(50.0));

        let json_without_value = r#"{}"#;
        let query: FanControlQuery = serde_json::from_str(json_without_value).unwrap();
        assert!(query.value.is_none());
    }

    #[test]
    fn test_fan_control_query_with_float_value() {
        let json = r#"{"value": 75.5}"#;
        let query: FanControlQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.value, Some(75.5));
    }

    #[test]
    fn test_fan_control_query_with_integer_value() {
        // Integers should parse as f64
        let json = r#"{"value": 100}"#;
        let query: FanControlQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.value, Some(100.0));
    }

    #[test]
    fn test_fan_id_board_validation() {
        use openfan_core::BoardType;

        let board_info = BoardType::OpenFanStandard.to_board_info();

        // Valid fan IDs (0-9 for Standard board)
        for fan_id in 0..10u8 {
            assert!(
                board_info.validate_fan_id(fan_id).is_ok(),
                "Fan ID {} should be valid",
                fan_id
            );
        }

        // Invalid fan ID (10 and above)
        assert!(board_info.validate_fan_id(10).is_err());
        assert!(board_info.validate_fan_id(255).is_err());
    }

    #[test]
    fn test_fan_control_query_negative_value() {
        let json = r#"{"value": -50.0}"#;
        let query: FanControlQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.value, Some(-50.0));
        // Note: Negative values are clamped to 0 in the handler
    }

    #[test]
    fn test_fan_control_query_large_value() {
        let json = r#"{"value": 9999.0}"#;
        let query: FanControlQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.value, Some(9999.0));
        // Note: For PWM, this is clamped to 100; for RPM, this fails validation
    }
}

/// Integration tests that exercise actual HTTP handlers
#[cfg(test)]
mod integration_tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        Router,
    };
    use http_body_util::BodyExt;
    use openfan_core::BoardType;
    use tower::ServiceExt;

    use crate::api::{create_router, AppState};
    use crate::config::RuntimeConfig;

    /// Create a test app with mock mode (no hardware)
    async fn create_test_app() -> Router {
        let board_info = BoardType::OpenFanStandard.to_board_info();
        let temp_dir = tempfile::tempdir().unwrap();
        // RuntimeConfig::load expects a path to a TOML config file
        let config_path = temp_dir.path().join("config.toml");
        let config = RuntimeConfig::load(&config_path).await.unwrap();
        let state = AppState::new(board_info, config, None);
        create_router(state)
    }

    /// Helper to extract response body as string
    async fn body_string(body: Body) -> String {
        let bytes = body.collect().await.unwrap().to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn test_get_fan_status_mock_mode() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/fan/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        // Verify it's a success response with mock data
        assert!(json.get("data").is_some());
        let data = json.get("data").unwrap();
        assert!(data.get("rpms").is_some());
        assert!(data.get("pwms").is_some());

        // Mock mode returns 10 fans for Standard board
        let rpms = data.get("rpms").unwrap().as_object().unwrap();
        assert_eq!(rpms.len(), 10);
    }

    #[tokio::test]
    async fn test_set_all_fans_missing_value() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/fan/all/set")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = body_string(response.into_body()).await;
        assert!(body.contains("Missing"));
    }

    #[tokio::test]
    async fn test_set_all_fans_valid_value() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/fan/all/set?value=50")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_set_fan_pwm_valid() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/fan/0/pwm?value=75")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_set_fan_pwm_invalid_fan_id() {
        let app = create_test_app().await;

        // Fan ID 10 is out of range for Standard board (0-9)
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/fan/10/pwm?value=50")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_set_fan_pwm_missing_value() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/fan/0/pwm")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_fan_rpm_mock_mode() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/fan/5/rpm/get")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        // Mock RPM for fan 5 should be 1500 + (5 * 100) = 2000
        let data = json.get("data").unwrap();
        assert_eq!(data.as_u64().unwrap(), 2000);
    }

    #[tokio::test]
    async fn test_get_fan_rpm_invalid_fan_id() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/fan/99/rpm/get")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_set_fan_rpm_valid() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/fan/0/rpm?value=1500")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_set_fan_rpm_below_minimum() {
        let app = create_test_app().await;

        // RPM below 500 should fail validation
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/fan/0/rpm?value=400")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_set_fan_rpm_above_maximum() {
        let app = create_test_app().await;

        // RPM above 9000 should fail validation
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/fan/0/rpm?value=10000")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_set_fan_rpm_boundary_values() {
        let app = create_test_app().await;

        // Minimum valid RPM (500)
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/fan/0/rpm?value=500")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Maximum valid RPM (9000)
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/fan/0/rpm?value=9000")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}

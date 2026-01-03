//! Fan handlers for status and control endpoints

use crate::api::error::ApiError;
use crate::api::AppState;
use crate::{api_fail, api_ok};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use openfan_core::api;
use serde::Deserialize;
use std::collections::HashMap;

use tracing::debug;

/// Query parameters for fan control endpoints.
#[derive(Deserialize)]
pub(crate) struct FanControlQuery {
    /// Value to set - PWM percentage (0-100) or target RPM (500-9000 per OpenFAN docs)
    pub value: Option<f64>,
}

/// Retrieves the current status of all fans for a specific controller.
///
/// # Endpoint
///
/// `GET /api/v0/controller/{id}/fan/status`
pub(crate) async fn get_controller_fan_status(
    State(state): State<AppState>,
    Path(controller_id): Path<String>,
) -> Result<Json<api::ApiResponse<api::FanStatusResponse>>, ApiError> {
    debug!(
        "Request: GET /api/v0/controller/{}/fan/status",
        controller_id
    );

    // Get controller from registry
    let entry = state
        .registry
        .get_or_err(&controller_id)
        .await
        .map_err(ApiError::from)?;

    let board_info = entry.board_info();

    // Check if hardware is available
    let Some(cm) = entry.connection_manager() else {
        debug!(
            "Controller '{}' is in mock mode - returning simulated fan status",
            controller_id
        );
        // Return mock fan data for testing/development
        let mut mock_rpms = HashMap::new();
        let mut mock_pwms = HashMap::new();
        for i in 0..board_info.fan_count as u8 {
            mock_rpms.insert(i, 1500 + (i as u32 * 100));
            mock_pwms.insert(i, 50 + (i as u32 * 5));
        }
        let mock_status = api::FanStatusResponse {
            rpms: mock_rpms,
            pwms: mock_pwms,
        };
        return api_ok!(mock_status);
    };

    // Get RPM and PWM data from hardware via connection manager
    let status = cm
        .with_controller(|controller| {
            Box::pin(async move {
                let rpm_map = controller.get_all_fan_rpm().await?;
                let pwm_map = controller.get_all_fan_pwm();
                debug!(
                    "Fan status retrieved - RPM: {:?}, PWM: {:?}",
                    rpm_map, pwm_map
                );
                Ok(api::FanStatusResponse {
                    rpms: rpm_map,
                    pwms: pwm_map,
                })
            })
        })
        .await?;

    api_ok!(status)
}

/// Sets the PWM value for all fans on a specific controller.
///
/// # Endpoint
///
/// `GET /api/v0/controller/{id}/fan/all/set?value=50`
pub(crate) async fn set_controller_all_fans(
    State(state): State<AppState>,
    Path(controller_id): Path<String>,
    Query(params): Query<FanControlQuery>,
) -> Result<Json<api::ApiResponse<()>>, ApiError> {
    debug!(
        "Request: GET /api/v0/controller/{}/fan/all/set",
        controller_id
    );

    let Some(value) = params.value else {
        return api_fail!("Missing 'value' parameter");
    };

    // Validate and clamp PWM value
    let pwm_value = value.clamp(0.0, 100.0) as u32;

    // Get controller from registry
    let entry = state
        .registry
        .get_or_err(&controller_id)
        .await
        .map_err(ApiError::from)?;

    debug!(
        "Setting all fans on controller '{}' to {}% PWM",
        controller_id, pwm_value
    );

    // Check if hardware is available
    let Some(cm) = entry.connection_manager() else {
        debug!(
            "Controller '{}' is in mock mode - simulating fan PWM set",
            controller_id
        );
        return api_ok!(());
    };

    // Send command to hardware via connection manager
    cm.with_controller(|controller| {
        Box::pin(async move {
            let response = controller.set_all_fan_pwm(pwm_value).await?;
            debug!("Set all fans response: {}", response);
            Ok(())
        })
    })
    .await?;

    api_ok!(())
}

/// Sets the PWM value for a specific fan on a controller.
///
/// # Endpoint
///
/// `GET /api/v0/controller/{id}/fan/{fan}/pwm?value=50`
pub(crate) async fn set_controller_fan_pwm(
    State(state): State<AppState>,
    Path((controller_id, fan_id)): Path<(String, String)>,
    Query(params): Query<FanControlQuery>,
) -> Result<Json<api::ApiResponse<()>>, ApiError> {
    debug!(
        "Request: GET /api/v0/controller/{}/fan/{}/pwm",
        controller_id, fan_id
    );

    // Parse and validate fan ID
    let fan_index = fan_id
        .parse::<u8>()
        .map_err(|_| ApiError::bad_request(format!("Invalid fan ID: {}", fan_id)))?;

    // Get controller from registry
    let entry = state
        .registry
        .get_or_err(&controller_id)
        .await
        .map_err(ApiError::from)?;

    // Validate fan ID against board configuration
    entry.board_info().validate_fan_id(fan_index)?;

    let Some(value) = params.value else {
        return api_fail!("Missing 'value' parameter");
    };

    // Validate and clamp PWM value
    let pwm_value = value.clamp(0.0, 100.0) as u32;

    debug!(
        "Setting fan {} on controller '{}' to {}% PWM",
        fan_index, controller_id, pwm_value
    );

    // Check if hardware is available
    let Some(cm) = entry.connection_manager() else {
        debug!(
            "Controller '{}' is in mock mode - simulating fan {} PWM set",
            controller_id, fan_index
        );
        return api_ok!(());
    };

    // Send command to hardware via connection manager
    cm.with_controller(|controller| {
        Box::pin(async move {
            let response = controller.set_fan_pwm(fan_index, pwm_value).await?;
            debug!("Set fan {} PWM response: {}", fan_index, response);
            Ok(())
        })
    })
    .await?;

    api_ok!(())
}

/// Retrieves the current RPM reading for a specific fan on a controller.
///
/// # Endpoint
///
/// `GET /api/v0/controller/{id}/fan/{fan}/rpm/get`
pub(crate) async fn get_controller_fan_rpm(
    State(state): State<AppState>,
    Path((controller_id, fan_id)): Path<(String, String)>,
) -> Result<Json<api::ApiResponse<u32>>, ApiError> {
    debug!(
        "Request: GET /api/v0/controller/{}/fan/{}/rpm/get",
        controller_id, fan_id
    );

    // Parse and validate fan ID
    let fan_index = fan_id
        .parse::<u8>()
        .map_err(|_| ApiError::bad_request(format!("Invalid fan ID: {}", fan_id)))?;

    // Get controller from registry
    let entry = state
        .registry
        .get_or_err(&controller_id)
        .await
        .map_err(ApiError::from)?;

    // Validate fan ID against board configuration
    entry.board_info().validate_fan_id(fan_index)?;

    // Check if hardware is available
    let Some(cm) = entry.connection_manager() else {
        debug!(
            "Controller '{}' is in mock mode - returning mock RPM for fan {}",
            controller_id, fan_index
        );
        let mock_rpm = 1500 + (fan_index as u32 * 100);
        return api_ok!(mock_rpm);
    };

    // Get single fan RPM from hardware via connection manager
    let rpm = cm
        .with_controller(|controller| {
            Box::pin(async move {
                let rpm = controller.get_single_fan_rpm(fan_index).await?;
                debug!("Fan {} RPM: {}", fan_index, rpm);
                Ok(rpm)
            })
        })
        .await?;

    api_ok!(rpm)
}

/// Sets the target RPM for a specific fan on a controller.
///
/// # Endpoint
///
/// `GET /api/v0/controller/{id}/fan/{fan}/rpm?value=1000`
pub(crate) async fn set_controller_fan_rpm(
    State(state): State<AppState>,
    Path((controller_id, fan_id)): Path<(String, String)>,
    Query(params): Query<FanControlQuery>,
) -> Result<Json<api::ApiResponse<()>>, ApiError> {
    debug!(
        "Request: GET /api/v0/controller/{}/fan/{}/rpm",
        controller_id, fan_id
    );

    // Parse and validate fan ID
    let fan_index = fan_id
        .parse::<u8>()
        .map_err(|_| ApiError::bad_request(format!("Invalid fan ID: {}", fan_id)))?;

    // Get controller from registry
    let entry = state
        .registry
        .get_or_err(&controller_id)
        .await
        .map_err(ApiError::from)?;

    // Validate fan ID against board configuration
    entry.board_info().validate_fan_id(fan_index)?;

    let Some(value) = params.value else {
        return api_fail!("Missing 'value' parameter");
    };

    // Convert to u32 and validate against board's target RPM range
    let rpm_value = value as u32;
    entry.board_info().validate_target_rpm(rpm_value)?;

    debug!(
        "Setting fan {} on controller '{}' to {} RPM",
        fan_index, controller_id, rpm_value
    );

    // Check if hardware is available
    let Some(cm) = entry.connection_manager() else {
        debug!(
            "Controller '{}' is in mock mode - simulating fan {} RPM set",
            controller_id, fan_index
        );
        return api_ok!(());
    };

    // Send command to hardware via connection manager
    cm.with_controller(|controller| {
        Box::pin(async move {
            let response = controller.set_fan_rpm(fan_index, rpm_value).await?;
            debug!("Set fan {} RPM response: {}", fan_index, response);
            Ok(())
        })
    })
    .await?;

    api_ok!(())
}

#[cfg(test)]
mod tests {
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
    use openfan_core::{config::StaticConfig, BoardType};
    use tower::ServiceExt;

    use crate::api::{create_router, AppState};
    use crate::config::RuntimeConfig;

    /// Create a test app with mock mode (no hardware)
    async fn create_test_app() -> Router {
        let board_info = BoardType::OpenFanStandard.to_board_info();
        let temp_dir = tempfile::tempdir().unwrap();
        // Create a config with data_dir inside the temp directory
        let config_path = temp_dir.path().join("config.toml");
        let static_config = StaticConfig::with_data_dir(temp_dir.path().join("data"));
        tokio::fs::write(&config_path, static_config.to_toml().unwrap())
            .await
            .unwrap();
        let config = RuntimeConfig::load(&config_path).await.unwrap();
        let state =
            AppState::single_controller(board_info, std::sync::Arc::new(config), None).await;
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
                    .uri("/api/v0/controller/default/fan/status")
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
                    .uri("/api/v0/controller/default/fan/all/set")
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
                    .uri("/api/v0/controller/default/fan/all/set?value=50")
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
                    .uri("/api/v0/controller/default/fan/0/pwm?value=75")
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
                    .uri("/api/v0/controller/default/fan/10/pwm?value=50")
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
                    .uri("/api/v0/controller/default/fan/0/pwm")
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
                    .uri("/api/v0/controller/default/fan/5/rpm/get")
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
                    .uri("/api/v0/controller/default/fan/99/rpm/get")
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
                    .uri("/api/v0/controller/default/fan/0/rpm?value=1500")
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
                    .uri("/api/v0/controller/default/fan/0/rpm?value=400")
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
                    .uri("/api/v0/controller/default/fan/0/rpm?value=10000")
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
                    .uri("/api/v0/controller/default/fan/0/rpm?value=500")
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
                    .uri("/api/v0/controller/default/fan/0/rpm?value=9000")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_set_fan_rpm_missing_value() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/default/fan/0/rpm")
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
    async fn test_set_fan_pwm_non_numeric_fan_id() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/default/fan/abc/pwm?value=50")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = body_string(response.into_body()).await;
        assert!(body.contains("Invalid fan ID"));
    }

    #[tokio::test]
    async fn test_get_fan_rpm_non_numeric_fan_id() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/default/fan/xyz/rpm/get")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = body_string(response.into_body()).await;
        assert!(body.contains("Invalid fan ID"));
    }

    #[tokio::test]
    async fn test_set_fan_rpm_non_numeric_fan_id() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/default/fan/foo/rpm?value=1500")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = body_string(response.into_body()).await;
        assert!(body.contains("Invalid fan ID"));
    }

    #[tokio::test]
    async fn test_fan_status_controller_not_found() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/nonexistent/fan/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_set_all_fans_controller_not_found() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/nonexistent/fan/all/set?value=50")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_set_fan_pwm_controller_not_found() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/nonexistent/fan/0/pwm?value=50")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_pwm_value_clamping() {
        let app = create_test_app().await;

        // Test value above 100 - should be clamped and succeed
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/default/fan/0/pwm?value=150")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Test negative value - should be clamped to 0 and succeed
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/default/fan/0/pwm?value=-10")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_set_all_fans_pwm_clamping() {
        let app = create_test_app().await;

        // Test value above 100 - should be clamped and succeed
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/default/fan/all/set?value=200")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Test negative value - should be clamped to 0 and succeed
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/default/fan/all/set?value=-50")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}

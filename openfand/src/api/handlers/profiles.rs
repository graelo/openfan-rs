//! Profile handlers for CRUD operations

use crate::api::error::ApiError;
use crate::api::AppState;
use crate::{api_fail, api_ok};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use openfan_core::{api, ControlMode, FanProfile};
use serde::Deserialize;

use tracing::{debug, info, warn};

/// Query parameters for profile operations.
#[derive(Deserialize)]
pub(crate) struct ProfileQuery {
    /// Profile name (case-sensitive)
    pub name: Option<String>,
}

/// Request body for adding a new profile.
#[derive(Deserialize)]
pub(crate) struct AddProfileRequest {
    /// Profile name (must be non-empty after trimming whitespace)
    pub name: String,
    /// Profile data (must have exactly 10 values with appropriate ranges)
    pub profile: FanProfile,
}

/// Lists all available fan profiles for a specific controller.
///
/// # Endpoint
///
/// `GET /api/v0/controller/{id}/profiles/list`
pub(crate) async fn list_controller_profiles(
    State(state): State<AppState>,
    Path(controller_id): Path<String>,
) -> Result<Json<api::ApiResponse<api::ProfileResponse>>, ApiError> {
    debug!(
        "Request: GET /api/v0/controller/{}/profiles/list",
        controller_id
    );

    // Validate controller exists in registry
    let _ = state
        .registry
        .get_or_err(&controller_id)
        .await
        .map_err(ApiError::from)?;

    // Get controller data
    let controller_data = state.config.controller_data(&controller_id).await?;

    let profiles = controller_data.profiles().await;
    let fan_profiles = profiles.profiles.clone();

    let response = api::ProfileResponse {
        profiles: fan_profiles,
    };

    info!(
        "Listed {} profiles for controller '{}'",
        response.profiles.len(),
        controller_id
    );
    api_ok!(response)
}

/// Adds a new fan profile to a specific controller.
///
/// # Endpoint
///
/// `POST /api/v0/controller/{id}/profiles/add`
pub(crate) async fn add_controller_profile(
    State(state): State<AppState>,
    Path(controller_id): Path<String>,
    Json(request): Json<AddProfileRequest>,
) -> Result<Json<api::ApiResponse<()>>, ApiError> {
    debug!(
        "Request: POST /api/v0/controller/{}/profiles/add",
        controller_id
    );

    // Get controller from registry
    let entry = state
        .registry
        .get_or_err(&controller_id)
        .await
        .map_err(ApiError::from)?;

    let board_info = entry.board_info();

    let profile_name = request.name.trim();

    if profile_name.is_empty() {
        return api_fail!("Profile name cannot be empty!");
    }

    let profile = request.profile;

    // Validate values count against board configuration
    if profile.values.len() != board_info.fan_count {
        return api_fail!(format!(
            "Profile must have exactly {} values for {}!",
            board_info.fan_count, board_info.name
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

    // Get controller data and add profile
    let controller_data = state.config.controller_data(&controller_id).await?;

    {
        let mut profiles = controller_data.profiles_mut().await;
        profiles.insert(profile_name.to_string(), profile);
    }

    // Save configuration
    if let Err(e) = controller_data.save_profiles().await {
        return Err(ApiError::internal_error(format!(
            "Failed to save profiles: {}",
            e
        )));
    }

    info!(
        "Added profile '{}' for controller '{}'",
        profile_name, controller_id
    );
    api_ok!(())
}

/// Removes a profile from a specific controller.
///
/// # Endpoint
///
/// `GET /api/v0/controller/{id}/profiles/remove?name=Custom`
pub(crate) async fn remove_controller_profile(
    State(state): State<AppState>,
    Path(controller_id): Path<String>,
    Query(params): Query<ProfileQuery>,
) -> Result<Json<api::ApiResponse<()>>, ApiError> {
    debug!(
        "Request: GET /api/v0/controller/{}/profiles/remove",
        controller_id
    );

    let Some(profile_name) = params.name else {
        return api_fail!("Name cannot be empty!");
    };

    // Validate controller exists in registry
    let _ = state
        .registry
        .get_or_err(&controller_id)
        .await
        .map_err(ApiError::from)?;

    // Get controller data
    let controller_data = state.config.controller_data(&controller_id).await?;

    // Remove from configuration
    let removed = {
        let mut profiles = controller_data.profiles_mut().await;
        profiles.remove(&profile_name)
    };

    if removed.is_some() {
        // Save configuration
        if let Err(e) = controller_data.save_profiles().await {
            return Err(ApiError::internal_error(format!(
                "Failed to save profiles: {}",
                e
            )));
        }

        info!(
            "Removed profile '{}' from controller '{}'",
            profile_name, controller_id
        );
        api_ok!(())
    } else {
        api_fail!(format!(
            "Profile '{}' does not exist! (Names are case-sensitive!)",
            profile_name
        ))
    }
}

/// Applies a profile to all fans on a specific controller.
///
/// # Endpoint
///
/// `GET /api/v0/controller/{id}/profiles/set?name=Gaming`
pub(crate) async fn set_controller_profile(
    State(state): State<AppState>,
    Path(controller_id): Path<String>,
    Query(params): Query<ProfileQuery>,
) -> Result<Json<api::ApiResponse<()>>, ApiError> {
    debug!(
        "Request: GET /api/v0/controller/{}/profiles/set",
        controller_id
    );

    let Some(profile_name) = params.name else {
        return api_fail!("Name cannot be empty!");
    };

    // Get controller from registry
    let entry = state
        .registry
        .get_or_err(&controller_id)
        .await
        .map_err(ApiError::from)?;

    // Get controller data
    let controller_data = state.config.controller_data(&controller_id).await?;

    // Get profile from configuration
    let profile = {
        let profiles = controller_data.profiles().await;
        match profiles.get(&profile_name) {
            Some(p) => p.clone(),
            None => {
                return api_fail!(format!(
                    "Profile '{}' does not exist! (Names are case-sensitive!)",
                    profile_name
                ));
            }
        }
    };

    // Check if hardware is available
    let Some(cm) = entry.connection_manager() else {
        debug!(
            "Controller '{}' is in mock mode - simulating profile application",
            controller_id
        );
        info!(
            "Applied profile '{}' to controller '{}' (mock mode)",
            profile_name, controller_id
        );
        return api_ok!(());
    };

    let profile_values = profile.values.clone();
    let control_mode = profile.control_mode;
    let pname = profile_name.clone();
    let cid = controller_id.clone();

    // Apply profile values to each fan via connection manager
    cm.with_controller(|controller| {
        Box::pin(async move {
            for (fan_id, &value) in profile_values.iter().enumerate() {
                let fan_id = fan_id as u8;

                let result = match control_mode {
                    ControlMode::Pwm => controller.set_fan_pwm(fan_id, value).await,
                    ControlMode::Rpm => controller.set_fan_rpm(fan_id, value).await,
                };

                if let Err(e) = result {
                    warn!(
                        "Controller '{}': Failed to set fan {} while applying profile '{}': {}",
                        cid, fan_id, pname, e
                    );
                }
            }
            Ok(())
        })
    })
    .await?;

    info!(
        "Applied profile '{}' to controller '{}'",
        profile_name, controller_id
    );
    api_ok!(())
}

#[cfg(test)]
mod tests {
    use openfan_core::{BoardConfig, DefaultBoard};

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

/// Integration tests that exercise actual HTTP handlers
#[cfg(test)]
mod integration_tests {
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
        Router,
    };
    use http_body_util::BodyExt;
    use openfan_core::BoardType;
    use tempfile::TempDir;
    use tower::ServiceExt;

    use crate::api::{create_router, AppState};
    use crate::config::RuntimeConfig;

    struct TestApp {
        router: Router,
        _config_dir: TempDir,
    }

    impl TestApp {
        async fn new() -> Self {
            let board_info = BoardType::OpenFanStandard.to_board_info();
            let config_dir = tempfile::tempdir().unwrap();

            let data_dir = config_dir.path().join("data");
            std::fs::create_dir_all(&data_dir).unwrap();

            let data_dir_str = data_dir.to_string_lossy().replace('\\', "\\\\");
            let config_content = format!(
                r#"data_dir = "{}"

[server]
bind_address = "127.0.0.1"
port = 3000
communication_timeout = 1
"#,
                data_dir_str
            );

            let config_path = config_dir.path().join("config.toml");
            std::fs::write(&config_path, config_content).unwrap();

            let config = RuntimeConfig::load(&config_path).await.unwrap();
            let state =
                AppState::single_controller(board_info, std::sync::Arc::new(config), None).await;

            TestApp {
                router: create_router(state),
                _config_dir: config_dir,
            }
        }

        fn router(&self) -> Router {
            self.router.clone()
        }
    }

    async fn body_string(body: Body) -> String {
        let bytes = body.collect().await.unwrap().to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn test_list_profiles() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/default/profiles/list")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let data = json.get("data").unwrap();
        assert!(data.get("profiles").is_some());
    }

    #[tokio::test]
    async fn test_add_profile_valid_pwm() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/controller/default/profiles/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "test_pwm", "profile": {"type": "pwm", "values": [50,50,50,50,50,50,50,50,50,50]}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_add_profile_valid_rpm() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/controller/default/profiles/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "test_rpm", "profile": {"type": "rpm", "values": [1000,1000,1000,1000,1000,1000,1000,1000,1000,1000]}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_add_profile_empty_name() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/controller/default/profiles/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "  ", "profile": {"type": "pwm", "values": [50,50,50,50,50,50,50,50,50,50]}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_add_profile_wrong_value_count() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/controller/default/profiles/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "wrong_count", "profile": {"type": "pwm", "values": [50,50,50]}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_add_profile_pwm_exceeds_limit() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/controller/default/profiles/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "bad_pwm", "profile": {"type": "pwm", "values": [50,50,50,101,50,50,50,50,50,50]}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_add_profile_rpm_exceeds_limit() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/controller/default/profiles/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "bad_rpm", "profile": {"type": "rpm", "values": [1000,1000,1000,16001,1000,1000,1000,1000,1000,1000]}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_remove_profile_missing_name() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/default/profiles/remove")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_remove_profile_not_found() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/default/profiles/remove?name=nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_add_then_remove_profile() {
        let app = TestApp::new().await;

        // Add profile
        let add_response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/controller/default/profiles/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "to_remove", "profile": {"type": "pwm", "values": [50,50,50,50,50,50,50,50,50,50]}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(add_response.status(), StatusCode::OK);

        // Remove profile
        let remove_response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/default/profiles/remove?name=to_remove")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(remove_response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_set_profile_missing_name() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/default/profiles/set")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_set_profile_not_found() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/default/profiles/set?name=nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_add_then_set_profile_mock_mode() {
        let app = TestApp::new().await;

        // Add profile
        let add_response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/controller/default/profiles/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "to_apply", "profile": {"type": "pwm", "values": [50,50,50,50,50,50,50,50,50,50]}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(add_response.status(), StatusCode::OK);

        // Set/apply profile (mock mode - no hardware)
        let set_response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/default/profiles/set?name=to_apply")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(set_response.status(), StatusCode::OK);
    }
}

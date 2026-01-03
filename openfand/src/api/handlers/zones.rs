//! Zone handlers for CRUD operations and coordinated fan control

use crate::api::error::ApiError;
use crate::api::AppState;
use crate::{api_fail, api_ok};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use openfan_core::{api, ControlMode, OpenFanError, Zone};
use serde::Deserialize;
use tracing::{debug, info, warn};

/// Query parameters for zone apply operation.
#[derive(Deserialize)]
pub(crate) struct ApplyZoneQuery {
    /// Control mode: "pwm" or "rpm"
    pub mode: String,
    /// Control value (0-100 for PWM, 0-16000 for RPM)
    pub value: u16,
}

/// Validates a zone name.
///
/// Valid names contain only alphanumeric characters, hyphens, and underscores.
fn is_valid_zone_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// Lists all configured zones.
///
/// # Endpoint
///
/// `GET /api/v0/zones/list`
pub(crate) async fn list_zones(
    State(state): State<AppState>,
) -> Result<Json<api::ApiResponse<api::ZoneResponse>>, ApiError> {
    debug!("Request: GET /api/v0/zones/list");

    let zones = state.config.zones().await;
    let zone_map = zones.zones.clone();

    let response = api::ZoneResponse { zones: zone_map };

    info!("Listed {} zones", response.zones.len());
    api_ok!(response)
}

/// Gets a single zone by name.
///
/// # Endpoint
///
/// `GET /api/v0/zone/{name}/get`
pub(crate) async fn get_zone(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<api::ApiResponse<api::SingleZoneResponse>>, ApiError> {
    debug!("Request: GET /api/v0/zone/{}/get", name);

    let zones = state.config.zones().await;

    match zones.get(&name) {
        Some(zone) => {
            let response = api::SingleZoneResponse { zone: zone.clone() };
            api_ok!(response)
        }
        None => Err(OpenFanError::ZoneNotFound(name).into()),
    }
}

/// Adds a new zone.
///
/// # Validation Rules
///
/// - Zone name must be non-empty and contain only alphanumeric characters, hyphens, and underscores
/// - Fan IDs must be valid for their respective controllers
/// - Fans must not be assigned to another zone (exclusive membership)
///
/// # Endpoint
///
/// `POST /api/v0/zones/add`
///
/// # Request Body
///
/// ```json
/// {
///   "name": "intake",
///   "fans": [
///     {"controller": "default", "fan_id": 0},
///     {"controller": "default", "fan_id": 1}
///   ],
///   "description": "Front intake fans"
/// }
/// ```
pub(crate) async fn add_zone(
    State(state): State<AppState>,
    Json(request): Json<api::AddZoneRequest>,
) -> Result<Json<api::ApiResponse<()>>, ApiError> {
    debug!("Request: POST /api/v0/zones/add");

    let zone_name = request.name.trim();

    if !is_valid_zone_name(zone_name) {
        return api_fail!(
            "Zone name must be non-empty and contain only alphanumeric characters, hyphens, and underscores!"
        );
    }

    // Validate fan IDs against board configuration
    // TODO: In multi-controller mode, validate against the controller's board info
    for fan in &request.fans {
        if let Err(e) = state.board_info.validate_fan_id(fan.fan_id) {
            return api_fail!(format!(
                "Invalid fan ID {} for controller '{}': {}",
                fan.fan_id, fan.controller, e
            ));
        }
    }

    // Check for duplicate fans in request
    let mut seen = std::collections::HashSet::new();
    for fan in &request.fans {
        if !seen.insert((&fan.controller, fan.fan_id)) {
            return api_fail!(format!(
                "Duplicate fan (controller: '{}', fan_id: {}) in request!",
                fan.controller, fan.fan_id
            ));
        }
    }

    // Check exclusive membership and add zone
    {
        let mut zones = state.config.zones_mut().await;

        // Check if zone name already exists
        if zones.contains(zone_name) {
            return api_fail!(format!("Zone '{}' already exists!", zone_name));
        }

        // Check for exclusive membership
        for fan in &request.fans {
            if let Some(existing_zone) = zones.find_zone_for_fan(&fan.controller, fan.fan_id) {
                return api_fail!(format!(
                    "Fan (controller: '{}', fan_id: {}) is already assigned to zone '{}'!",
                    fan.controller, fan.fan_id, existing_zone
                ));
            }
        }

        // Create and insert the zone
        let zone = if let Some(desc) = request.description {
            Zone::with_description(zone_name, request.fans, desc)
        } else {
            Zone::new(zone_name, request.fans)
        };

        zones.insert(zone_name.to_string(), zone);
    }

    // Save configuration
    if let Err(e) = state.config.save_zones().await {
        return Err(ApiError::internal_error(format!(
            "Failed to save configuration: {}",
            e
        )));
    }

    info!("Added zone: {}", zone_name);
    api_ok!(())
}

/// Updates an existing zone.
///
/// # Validation Rules
///
/// - Zone must exist
/// - Fan IDs must be valid for their respective controllers
/// - Fans must not be assigned to another zone (exclusive membership)
///
/// # Endpoint
///
/// `POST /api/v0/zone/{name}/update`
///
/// # Request Body
///
/// ```json
/// {
///   "fans": [
///     {"controller": "default", "fan_id": 0},
///     {"controller": "default", "fan_id": 1}
///   ],
///   "description": "Updated description"
/// }
/// ```
pub(crate) async fn update_zone(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(request): Json<api::UpdateZoneRequest>,
) -> Result<Json<api::ApiResponse<()>>, ApiError> {
    debug!("Request: POST /api/v0/zone/{}/update", name);

    // Validate fan IDs against board configuration
    // TODO: In multi-controller mode, validate against the controller's board info
    for fan in &request.fans {
        if let Err(e) = state.board_info.validate_fan_id(fan.fan_id) {
            return api_fail!(format!(
                "Invalid fan ID {} for controller '{}': {}",
                fan.fan_id, fan.controller, e
            ));
        }
    }

    // Check for duplicate fans in request
    let mut seen = std::collections::HashSet::new();
    for fan in &request.fans {
        if !seen.insert((&fan.controller, fan.fan_id)) {
            return api_fail!(format!(
                "Duplicate fan (controller: '{}', fan_id: {}) in request!",
                fan.controller, fan.fan_id
            ));
        }
    }

    // Update the zone
    {
        let mut zones = state.config.zones_mut().await;

        // Check if zone exists
        if !zones.contains(&name) {
            return Err(OpenFanError::ZoneNotFound(name).into());
        }

        // Check for exclusive membership (excluding fans already in this zone)
        for fan in &request.fans {
            if let Some(existing_zone) = zones.find_zone_for_fan(&fan.controller, fan.fan_id) {
                if existing_zone != name {
                    return api_fail!(format!(
                        "Fan (controller: '{}', fan_id: {}) is already assigned to zone '{}'!",
                        fan.controller, fan.fan_id, existing_zone
                    ));
                }
            }
        }

        // Update the zone
        let zone = if let Some(desc) = request.description {
            Zone::with_description(&name, request.fans, desc)
        } else {
            Zone::new(&name, request.fans)
        };

        zones.insert(name.clone(), zone);
    }

    // Save configuration
    if let Err(e) = state.config.save_zones().await {
        return Err(ApiError::internal_error(format!(
            "Failed to save configuration: {}",
            e
        )));
    }

    info!("Updated zone: {}", name);
    api_ok!(())
}

/// Deletes a zone.
///
/// # Endpoint
///
/// `GET /api/v0/zone/{name}/delete`
pub(crate) async fn delete_zone(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<api::ApiResponse<()>>, ApiError> {
    debug!("Request: GET /api/v0/zone/{}/delete", name);

    // Remove the zone
    let removed = {
        let mut zones = state.config.zones_mut().await;
        zones.remove(&name)
    };

    if removed.is_some() {
        // Save configuration
        if let Err(e) = state.config.save_zones().await {
            return Err(ApiError::internal_error(format!(
                "Failed to save configuration: {}",
                e
            )));
        }

        info!("Deleted zone: {}", name);
        api_ok!(())
    } else {
        Err(OpenFanError::ZoneNotFound(name).into())
    }
}

/// Applies a PWM or RPM value to all fans in a zone.
///
/// # Endpoint
///
/// `GET /api/v0/zone/{name}/apply?mode=pwm&value=75`
///
/// # Query Parameters
///
/// - `mode` - Control mode: "pwm" or "rpm"
/// - `value` - Control value (0-100 for PWM, 0-16000 for RPM)
pub(crate) async fn apply_zone(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(params): Query<ApplyZoneQuery>,
) -> Result<Json<api::ApiResponse<()>>, ApiError> {
    debug!(
        "Request: GET /api/v0/zone/{}/apply?mode={}&value={}",
        name, params.mode, params.value
    );

    // Parse control mode
    let mode = match params.mode.to_lowercase().as_str() {
        "pwm" => ControlMode::Pwm,
        "rpm" => ControlMode::Rpm,
        _ => {
            return api_fail!(format!(
                "Invalid mode '{}'. Must be 'pwm' or 'rpm'.",
                params.mode
            ));
        }
    };

    // Validate value range
    match mode {
        ControlMode::Pwm => {
            if params.value > 100 {
                return api_fail!(format!(
                    "PWM value {} exceeds maximum of 100!",
                    params.value
                ));
            }
        }
        ControlMode::Rpm => {
            if params.value > 16000 {
                return api_fail!(format!(
                    "RPM value {} exceeds maximum of 16000!",
                    params.value
                ));
            }
        }
    }

    // Get the zone
    let zone = {
        let zones = state.config.zones().await;
        match zones.get(&name) {
            Some(z) => z.clone(),
            None => {
                return Err(OpenFanError::ZoneNotFound(name).into());
            }
        }
    };

    if zone.fans.is_empty() {
        return api_fail!(format!("Zone '{}' has no fans assigned!", name));
    }

    // Check if hardware is available
    let Some(cm) = &state.connection_manager else {
        debug!("Hardware not available - simulating zone application for testing");
        info!(
            "Applied {} {} to zone '{}' (mock mode)",
            params.value,
            params.mode.to_uppercase(),
            name
        );
        return api_ok!(());
    };

    let value = params.value as u32;
    // For now, in single-controller mode, we just extract the fan_ids
    // TODO: In multi-controller mode, group fans by controller and apply to each
    let fan_ids: Vec<u8> = zone.fans.iter().map(|f| f.fan_id).collect();
    let zone_name = name.clone();

    // Apply value to each fan in the zone via connection manager
    cm.with_controller(|controller| {
        Box::pin(async move {
            for &fan_id in &fan_ids {
                let result = match mode {
                    ControlMode::Pwm => controller.set_fan_pwm(fan_id, value).await,
                    ControlMode::Rpm => controller.set_fan_rpm(fan_id, value).await,
                };

                if let Err(e) = result {
                    warn!(
                        "Failed to set fan {} in zone '{}': {}",
                        fan_id, zone_name, e
                    );
                }
            }
            Ok(())
        })
    })
    .await?;

    info!(
        "Applied {} {} to {} fans in zone '{}'",
        params.value,
        params.mode.to_uppercase(),
        zone.fans.len(),
        name
    );
    api_ok!(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_zone_names() {
        assert!(is_valid_zone_name("intake"));
        assert!(is_valid_zone_name("front-intake"));
        assert!(is_valid_zone_name("cpu_fans"));
        assert!(is_valid_zone_name("Zone1"));
        assert!(is_valid_zone_name("my-zone_2"));
    }

    #[test]
    fn test_invalid_zone_names() {
        assert!(!is_valid_zone_name(""));
        assert!(!is_valid_zone_name("zone with spaces"));
        assert!(!is_valid_zone_name("zone.name"));
        assert!(!is_valid_zone_name("zone#1"));
        assert!(!is_valid_zone_name("zone@test"));
    }

    #[test]
    fn test_apply_zone_query_parsing() {
        // This test verifies the query parameter structure
        let query = ApplyZoneQuery {
            mode: "pwm".to_string(),
            value: 75,
        };
        assert_eq!(query.mode, "pwm");
        assert_eq!(query.value, 75);
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
    async fn test_list_zones() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/zones/list")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let data = json.get("data").unwrap();
        assert!(data.get("zones").is_some());
    }

    #[tokio::test]
    async fn test_add_zone_valid() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/zones/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "intake", "fans": [{"controller": "default", "fan_id": 0}, {"controller": "default", "fan_id": 1}, {"controller": "default", "fan_id": 2}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_add_zone_with_description() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/zones/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "exhaust", "fans": [{"controller": "default", "fan_id": 3}, {"controller": "default", "fan_id": 4}], "description": "Rear exhaust fans"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_add_zone_invalid_name() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/zones/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "invalid zone name", "fans": [{"controller": "default", "fan_id": 0}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_add_zone_invalid_port() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/zones/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "bad-zone", "fans": [{"controller": "default", "fan_id": 99}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_add_zone_duplicate_port_in_request() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/zones/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "dup-zone", "fans": [{"controller": "default", "fan_id": 0}, {"controller": "default", "fan_id": 0}, {"controller": "default", "fan_id": 1}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_zone_not_found() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/zone/nonexistent/get")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_add_then_get_zone() {
        let app = TestApp::new().await;

        // Add zone
        let add_response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/zones/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "test-zone", "fans": [{"controller": "default", "fan_id": 5}, {"controller": "default", "fan_id": 6}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(add_response.status(), StatusCode::OK);

        // Get zone
        let get_response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/zone/test-zone/get")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_delete_zone_not_found() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/zone/nonexistent/delete")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_add_then_delete_zone() {
        let app = TestApp::new().await;

        // Add zone
        let add_response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/zones/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "to-delete", "fans": [{"controller": "default", "fan_id": 7}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(add_response.status(), StatusCode::OK);

        // Delete zone
        let delete_response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/zone/to-delete/delete")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(delete_response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_apply_zone_not_found() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/zone/nonexistent/apply?mode=pwm&value=50")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_apply_zone_invalid_mode() {
        let app = TestApp::new().await;

        // First add a zone
        let _ = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/zones/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "mode-test", "fans": [{"controller": "default", "fan_id": 8}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/zone/mode-test/apply?mode=invalid&value=50")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_apply_zone_pwm_value_too_high() {
        let app = TestApp::new().await;

        // First add a zone
        let _ = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/zones/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "pwm-test", "fans": [{"controller": "default", "fan_id": 9}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/zone/pwm-test/apply?mode=pwm&value=101")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_add_then_apply_zone_mock_mode() {
        let app = TestApp::new().await;

        // Add zone
        let add_response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/zones/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "apply-test", "fans": [{"controller": "default", "fan_id": 0}, {"controller": "default", "fan_id": 1}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(add_response.status(), StatusCode::OK);

        // Apply zone (mock mode)
        let apply_response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/zone/apply-test/apply?mode=pwm&value=75")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(apply_response.status(), StatusCode::OK);
    }
}

/// Multi-controller integration tests
#[cfg(test)]
mod multi_controller_tests {
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
        Router,
    };
    use http_body_util::BodyExt;
    use openfan_core::BoardType;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tower::ServiceExt;

    use crate::api::{create_router, AppState};
    use crate::config::RuntimeConfig;
    use crate::hardware::{ControllerEntry, ControllerRegistry};

    struct MultiControllerTestApp {
        router: Router,
        _config_dir: TempDir,
    }

    impl MultiControllerTestApp {
        /// Create a test app with multiple mock controllers
        async fn new() -> Self {
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

            // Create registry with multiple controllers
            let registry = ControllerRegistry::new();

            // Main controller: standard board (10 fans)
            let main_board = BoardType::OpenFanStandard.to_board_info();
            let main_entry =
                ControllerEntry::with_description("main", main_board.clone(), None, "Main chassis");
            registry.register(main_entry).await.unwrap();

            // GPU controller: custom board (4 fans)
            let gpu_board = BoardType::Custom { fan_count: 4 }.to_board_info();
            let gpu_entry =
                ControllerEntry::with_description("gpu", gpu_board, None, "GPU cooling");
            registry.register(gpu_entry).await.unwrap();

            let state = AppState::new(
                Arc::new(registry),
                Arc::new(config),
                main_board, // default controller is "main"
                None,       // mock mode
            );

            MultiControllerTestApp {
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
    async fn test_list_multiple_controllers() {
        let app = MultiControllerTestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controllers")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let data = json.get("data").unwrap();
        let controllers = data.get("controllers").unwrap().as_array().unwrap();

        assert_eq!(controllers.len(), 2);

        // Check both controllers are present
        let ids: Vec<&str> = controllers
            .iter()
            .map(|c| c.get("id").unwrap().as_str().unwrap())
            .collect();
        assert!(ids.contains(&"main"));
        assert!(ids.contains(&"gpu"));
    }

    #[tokio::test]
    async fn test_get_controller_info() {
        let app = MultiControllerTestApp::new().await;

        // Get main controller info
        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/main/info")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let data = json.get("data").unwrap();

        assert_eq!(data.get("id").unwrap().as_str().unwrap(), "main");
        assert_eq!(
            data.get("description").unwrap().as_str().unwrap(),
            "Main chassis"
        );
        assert_eq!(data.get("fan_count").unwrap().as_u64().unwrap(), 10);
    }

    #[tokio::test]
    async fn test_get_controller_info_gpu() {
        let app = MultiControllerTestApp::new().await;

        // Get GPU controller info
        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/gpu/info")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let data = json.get("data").unwrap();

        assert_eq!(data.get("id").unwrap().as_str().unwrap(), "gpu");
        assert_eq!(
            data.get("description").unwrap().as_str().unwrap(),
            "GPU cooling"
        );
        assert_eq!(data.get("fan_count").unwrap().as_u64().unwrap(), 4);
    }

    #[tokio::test]
    async fn test_controller_not_found() {
        let app = MultiControllerTestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/nonexistent/info")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_cross_controller_zone_creation() {
        let app = MultiControllerTestApp::new().await;

        // Create a zone spanning both controllers
        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/zones/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{
                            "name": "all-intake",
                            "fans": [
                                {"controller": "main", "fan_id": 0},
                                {"controller": "main", "fan_id": 1},
                                {"controller": "gpu", "fan_id": 0},
                                {"controller": "gpu", "fan_id": 1}
                            ],
                            "description": "All intake fans across controllers"
                        }"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_cross_controller_zone_get() {
        let app = MultiControllerTestApp::new().await;

        // Create a cross-controller zone first
        let _ = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/zones/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{
                            "name": "mixed-zone",
                            "fans": [
                                {"controller": "main", "fan_id": 2},
                                {"controller": "gpu", "fan_id": 2}
                            ]
                        }"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Retrieve the zone
        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/zone/mixed-zone/get")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let zone = json.get("data").unwrap().get("zone").unwrap();
        let fans = zone.get("fans").unwrap().as_array().unwrap();

        assert_eq!(fans.len(), 2);

        // Verify fans from different controllers
        let controllers: Vec<&str> = fans
            .iter()
            .map(|f| f.get("controller").unwrap().as_str().unwrap())
            .collect();
        assert!(controllers.contains(&"main"));
        assert!(controllers.contains(&"gpu"));
    }

    #[tokio::test]
    async fn test_cross_controller_zone_apply() {
        let app = MultiControllerTestApp::new().await;

        // Create a cross-controller zone
        let add_response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/zones/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{
                            "name": "apply-cross",
                            "fans": [
                                {"controller": "main", "fan_id": 3},
                                {"controller": "gpu", "fan_id": 3}
                            ]
                        }"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(add_response.status(), StatusCode::OK);

        // Apply PWM to the cross-controller zone (mock mode)
        let apply_response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/zone/apply-cross/apply?mode=pwm&value=60")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(apply_response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_zone_exclusive_membership_cross_controller() {
        let app = MultiControllerTestApp::new().await;

        // Create first zone with fans from both controllers
        let first_response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/zones/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{
                            "name": "first-zone",
                            "fans": [
                                {"controller": "main", "fan_id": 4},
                                {"controller": "gpu", "fan_id": 0}
                            ]
                        }"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first_response.status(), StatusCode::OK);

        // Try to create second zone with overlapping gpu fan
        let second_response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/zones/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{
                            "name": "second-zone",
                            "fans": [
                                {"controller": "main", "fan_id": 5},
                                {"controller": "gpu", "fan_id": 0}
                            ]
                        }"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should fail due to exclusive membership
        assert_eq!(second_response.status(), StatusCode::BAD_REQUEST);

        let body = body_string(second_response.into_body()).await;
        assert!(body.contains("already assigned"));
    }

    #[tokio::test]
    async fn test_controller_scoped_fan_status() {
        let app = MultiControllerTestApp::new().await;

        // Get fan status for main controller (mock mode returns simulated data)
        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/main/fan/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let data = json.get("data").unwrap();

        // Mock mode returns rpms and pwms for all fans on the controller
        let rpms = data.get("rpms").unwrap().as_object().unwrap();
        let pwms = data.get("pwms").unwrap().as_object().unwrap();

        // Main controller has 10 fans (standard board)
        assert_eq!(rpms.len(), 10);
        assert_eq!(pwms.len(), 10);
    }

    #[tokio::test]
    async fn test_controller_scoped_fan_status_gpu() {
        let app = MultiControllerTestApp::new().await;

        // Get fan status for GPU controller
        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/gpu/fan/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let data = json.get("data").unwrap();

        // Mock mode returns rpms and pwms for all fans on the controller
        let rpms = data.get("rpms").unwrap().as_object().unwrap();
        let pwms = data.get("pwms").unwrap().as_object().unwrap();

        // GPU controller has 4 fans (custom:4 board)
        assert_eq!(rpms.len(), 4);
        assert_eq!(pwms.len(), 4);
    }

    #[tokio::test]
    async fn test_zone_update_cross_controller() {
        let app = MultiControllerTestApp::new().await;

        // Create initial zone
        let _ = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/zones/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{
                            "name": "update-zone",
                            "fans": [{"controller": "main", "fan_id": 6}]
                        }"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Update to include fans from both controllers
        let update_response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/zone/update-zone/update")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{
                            "fans": [
                                {"controller": "main", "fan_id": 6},
                                {"controller": "main", "fan_id": 7},
                                {"controller": "gpu", "fan_id": 1}
                            ],
                            "description": "Updated to cross-controller"
                        }"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(update_response.status(), StatusCode::OK);

        // Verify the update
        let get_response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/zone/update-zone/get")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = body_string(get_response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let zone = json.get("data").unwrap().get("zone").unwrap();
        let fans = zone.get("fans").unwrap().as_array().unwrap();

        assert_eq!(fans.len(), 3);
        assert_eq!(
            zone.get("description").unwrap().as_str().unwrap(),
            "Updated to cross-controller"
        );
    }
}

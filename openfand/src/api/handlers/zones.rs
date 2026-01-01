//! Zone handlers for CRUD operations and coordinated fan control

use crate::api::error::ApiError;
use crate::api::AppState;
use crate::{api_fail, api_ok};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use openfan_core::{api, ControlMode, Zone};
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
/// `GET /api/v0/zone/:name/get`
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
        None => api_fail!(format!(
            "Zone '{}' does not exist! (Names are case-sensitive!)",
            name
        )),
    }
}

/// Adds a new zone.
///
/// # Validation Rules
///
/// - Zone name must be non-empty and contain only alphanumeric characters, hyphens, and underscores
/// - Port IDs must be valid for the detected board
/// - Port IDs must not be assigned to another zone (exclusive membership)
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
///   "port_ids": [0, 1, 2],
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

    // Validate port IDs against board configuration
    for &port_id in &request.port_ids {
        if let Err(e) = state.board_info.validate_fan_id(port_id) {
            return api_fail!(format!("Invalid port ID {}: {}", port_id, e));
        }
    }

    // Check for duplicate port IDs in request
    let mut seen = std::collections::HashSet::new();
    for &port_id in &request.port_ids {
        if !seen.insert(port_id) {
            return api_fail!(format!("Duplicate port ID {} in request!", port_id));
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
        for &port_id in &request.port_ids {
            if let Some(existing_zone) = zones.find_zone_for_port(port_id) {
                return api_fail!(format!(
                    "Port {} is already assigned to zone '{}'!",
                    port_id, existing_zone
                ));
            }
        }

        // Create and insert the zone
        let zone = if let Some(desc) = request.description {
            Zone::with_description(zone_name, request.port_ids, desc)
        } else {
            Zone::new(zone_name, request.port_ids)
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
/// - Port IDs must be valid for the detected board
/// - Port IDs must not be assigned to another zone (exclusive membership)
///
/// # Endpoint
///
/// `POST /api/v0/zone/:name/update`
///
/// # Request Body
///
/// ```json
/// {
///   "port_ids": [0, 1, 2, 3],
///   "description": "Updated description"
/// }
/// ```
pub(crate) async fn update_zone(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(request): Json<api::UpdateZoneRequest>,
) -> Result<Json<api::ApiResponse<()>>, ApiError> {
    debug!("Request: POST /api/v0/zone/{}/update", name);

    // Validate port IDs against board configuration
    for &port_id in &request.port_ids {
        if let Err(e) = state.board_info.validate_fan_id(port_id) {
            return api_fail!(format!("Invalid port ID {}: {}", port_id, e));
        }
    }

    // Check for duplicate port IDs in request
    let mut seen = std::collections::HashSet::new();
    for &port_id in &request.port_ids {
        if !seen.insert(port_id) {
            return api_fail!(format!("Duplicate port ID {} in request!", port_id));
        }
    }

    // Update the zone
    {
        let mut zones = state.config.zones_mut().await;

        // Check if zone exists
        if !zones.contains(&name) {
            return api_fail!(format!(
                "Zone '{}' does not exist! (Names are case-sensitive!)",
                name
            ));
        }

        // Check for exclusive membership (excluding ports already in this zone)
        for &port_id in &request.port_ids {
            if let Some(existing_zone) = zones.find_zone_for_port(port_id) {
                if existing_zone != name {
                    return api_fail!(format!(
                        "Port {} is already assigned to zone '{}'!",
                        port_id, existing_zone
                    ));
                }
            }
        }

        // Update the zone
        let zone = if let Some(desc) = request.description {
            Zone::with_description(&name, request.port_ids, desc)
        } else {
            Zone::new(&name, request.port_ids)
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
/// `GET /api/v0/zone/:name/delete`
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
        api_fail!(format!(
            "Zone '{}' does not exist! (Names are case-sensitive!)",
            name
        ))
    }
}

/// Applies a PWM or RPM value to all fans in a zone.
///
/// # Endpoint
///
/// `GET /api/v0/zone/:name/apply?mode=pwm&value=75`
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
                return api_fail!(format!(
                    "Zone '{}' does not exist! (Names are case-sensitive!)",
                    name
                ));
            }
        }
    };

    if zone.port_ids.is_empty() {
        return api_fail!(format!("Zone '{}' has no fans assigned!", name));
    }

    // Check if hardware is available
    let Some(fan_controller) = &state.fan_controller else {
        debug!("Hardware not available - simulating zone application for testing");
        info!(
            "Applied {} {} to zone '{}' (mock mode)",
            params.value,
            params.mode.to_uppercase(),
            name
        );
        return api_ok!(());
    };

    let mut controller = fan_controller.lock().await;
    let value = params.value as u32;

    // Apply value to each fan in the zone
    for &fan_id in &zone.port_ids {
        let result = match mode {
            ControlMode::Pwm => controller.set_fan_pwm(fan_id, value).await,
            ControlMode::Rpm => controller.set_fan_rpm(fan_id, value).await,
        };

        if let Err(e) = result {
            warn!("Failed to set fan {} in zone '{}': {}", fan_id, name, e);
        }
    }

    info!(
        "Applied {} {} to {} fans in zone '{}'",
        params.value,
        params.mode.to_uppercase(),
        zone.port_ids.len(),
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
hostname = "localhost"
port = 3000
communication_timeout = 1

[hardware]
hostname = "localhost"
port = 3000
communication_timeout = 1
"#,
                data_dir_str
            );

            let config_path = config_dir.path().join("config.toml");
            std::fs::write(&config_path, config_content).unwrap();

            let config = RuntimeConfig::load(&config_path).await.unwrap();
            let state = AppState::new(board_info, config, None);

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
                    .body(Body::from(r#"{"name": "intake", "port_ids": [0, 1, 2]}"#))
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
                        r#"{"name": "exhaust", "port_ids": [3, 4], "description": "Rear exhaust fans"}"#,
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
                        r#"{"name": "invalid zone name", "port_ids": [0]}"#,
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
                    .body(Body::from(r#"{"name": "bad-zone", "port_ids": [99]}"#))
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
                    .body(Body::from(r#"{"name": "dup-zone", "port_ids": [0, 0, 1]}"#))
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
                    .body(Body::from(r#"{"name": "test-zone", "port_ids": [5, 6]}"#))
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
                    .body(Body::from(r#"{"name": "to-delete", "port_ids": [7]}"#))
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
                    .body(Body::from(r#"{"name": "mode-test", "port_ids": [8]}"#))
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
                    .body(Body::from(r#"{"name": "pwm-test", "port_ids": [9]}"#))
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
                    .body(Body::from(r#"{"name": "apply-test", "port_ids": [0, 1]}"#))
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

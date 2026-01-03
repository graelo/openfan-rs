//! CFM mapping handlers for display-only airflow information

use crate::api::error::ApiError;
use crate::api::AppState;
use crate::{api_fail, api_ok};
use axum::{
    extract::{Path, State},
    Json,
};
use openfan_core::config::CfmMappingData;
use openfan_core::{api, OpenFanError};
use tracing::{debug, info};

/// List all CFM mappings for a specific controller.
///
/// # Endpoint
///
/// `GET /api/v0/controller/{id}/cfm/list`
pub(crate) async fn list_controller_cfm(
    State(state): State<AppState>,
    Path(controller_id): Path<String>,
) -> Result<Json<api::ApiResponse<api::CfmListResponse>>, ApiError> {
    debug!("Request: GET /api/v0/controller/{}/cfm/list", controller_id);

    // Validate controller exists in registry
    let _ = state
        .registry
        .get_or_err(&controller_id)
        .await
        .map_err(ApiError::from)?;

    // Get controller data
    let controller_data = state.config.controller_data(&controller_id).await?;

    let cfm_data = controller_data.cfm_mappings().await;
    let mappings = cfm_data.mappings.clone();

    let response = api::CfmListResponse { mappings };

    info!(
        "Listed {} CFM mappings for controller '{}'",
        response.mappings.len(),
        controller_id
    );
    api_ok!(response)
}

/// Get CFM mapping for a specific port on a controller.
///
/// # Endpoint
///
/// `GET /api/v0/controller/{id}/cfm/{port}`
pub(crate) async fn get_controller_cfm(
    State(state): State<AppState>,
    Path((controller_id, port)): Path<(String, String)>,
) -> Result<Json<api::ApiResponse<api::CfmGetResponse>>, ApiError> {
    debug!(
        "Request: GET /api/v0/controller/{}/cfm/{}",
        controller_id, port
    );

    // Parse and validate port ID
    let port_id = port
        .parse::<u8>()
        .map_err(|_| ApiError::bad_request(format!("Invalid port ID: {}", port)))?;

    // Get controller from registry
    let entry = state
        .registry
        .get_or_err(&controller_id)
        .await
        .map_err(ApiError::from)?;

    // Validate port ID against board configuration
    entry.board_info().validate_fan_id(port_id)?;

    // Get controller data
    let controller_data = state.config.controller_data(&controller_id).await?;

    let cfm_data = controller_data.cfm_mappings().await;

    match cfm_data.get(port_id) {
        Some(cfm) => {
            let response = api::CfmGetResponse {
                port: port_id,
                cfm_at_100: cfm,
            };
            api_ok!(response)
        }
        None => Err(OpenFanError::CfmMappingNotFound(port_id).into()),
    }
}

/// Set CFM mapping for a specific port on a controller.
///
/// # Endpoint
///
/// `POST /api/v0/controller/{id}/cfm/{port}`
pub(crate) async fn set_controller_cfm(
    State(state): State<AppState>,
    Path((controller_id, port)): Path<(String, String)>,
    Json(request): Json<api::SetCfmRequest>,
) -> Result<Json<api::ApiResponse<()>>, ApiError> {
    debug!(
        "Request: POST /api/v0/controller/{}/cfm/{}",
        controller_id, port
    );

    // Parse and validate port ID
    let port_id = port
        .parse::<u8>()
        .map_err(|_| ApiError::bad_request(format!("Invalid port ID: {}", port)))?;

    // Get controller from registry
    let entry = state
        .registry
        .get_or_err(&controller_id)
        .await
        .map_err(ApiError::from)?;

    // Validate port ID against board configuration
    entry.board_info().validate_fan_id(port_id)?;

    // Validate CFM value
    if let Err(e) = CfmMappingData::validate_cfm(request.cfm_at_100) {
        return api_fail!(e);
    }

    // Get controller data
    let controller_data = state.config.controller_data(&controller_id).await?;

    // Update configuration
    {
        let mut cfm_data = controller_data.cfm_mappings_mut().await;
        cfm_data.set(port_id, request.cfm_at_100);
    }

    // Save configuration
    if let Err(e) = controller_data.save_cfm_mappings().await {
        return Err(ApiError::internal_error(format!(
            "Failed to save configuration: {}",
            e
        )));
    }

    info!(
        "Controller '{}': Set CFM mapping for port {} to {}",
        controller_id, port_id, request.cfm_at_100
    );
    api_ok!(())
}

/// Delete CFM mapping for a specific port on a controller.
///
/// # Endpoint
///
/// `DELETE /api/v0/controller/{id}/cfm/{port}`
pub(crate) async fn delete_controller_cfm(
    State(state): State<AppState>,
    Path((controller_id, port)): Path<(String, String)>,
) -> Result<Json<api::ApiResponse<()>>, ApiError> {
    debug!(
        "Request: DELETE /api/v0/controller/{}/cfm/{}",
        controller_id, port
    );

    // Parse and validate port ID
    let port_id = port
        .parse::<u8>()
        .map_err(|_| ApiError::bad_request(format!("Invalid port ID: {}", port)))?;

    // Get controller from registry
    let entry = state
        .registry
        .get_or_err(&controller_id)
        .await
        .map_err(ApiError::from)?;

    // Validate port ID against board configuration
    entry.board_info().validate_fan_id(port_id)?;

    // Get controller data
    let controller_data = state.config.controller_data(&controller_id).await?;

    // Check if mapping exists
    let existed = {
        let mut cfm_data = controller_data.cfm_mappings_mut().await;
        cfm_data.remove(port_id)
    };

    if !existed {
        return Err(OpenFanError::CfmMappingNotFound(port_id).into());
    }

    // Save configuration
    if let Err(e) = controller_data.save_cfm_mappings().await {
        return Err(ApiError::internal_error(format!(
            "Failed to save configuration: {}",
            e
        )));
    }

    info!(
        "Controller '{}': Deleted CFM mapping for port {}",
        controller_id, port_id
    );
    api_ok!(())
}

#[cfg(test)]
mod tests {
    use openfan_core::config::CfmMappingData;

    #[test]
    fn test_port_id_parsing() {
        assert_eq!("0".parse::<u8>().unwrap(), 0);
        assert_eq!("9".parse::<u8>().unwrap(), 9);
        assert!("abc".parse::<u8>().is_err());
        assert!("-1".parse::<u8>().is_err());
    }

    #[test]
    fn test_port_id_parsing_edge_cases() {
        // Valid port IDs for standard board (0-9)
        for i in 0..=9u8 {
            assert!(i.to_string().parse::<u8>().is_ok());
        }

        // Large values still parse as u8 but would fail board validation
        assert_eq!("255".parse::<u8>().unwrap(), 255);

        // Invalid string formats
        assert!("".parse::<u8>().is_err());
        assert!("1.5".parse::<u8>().is_err());
        assert!(" 5".parse::<u8>().is_err());
        assert!("0x05".parse::<u8>().is_err());
    }

    #[test]
    fn test_cfm_validation_valid_values() {
        // Valid CFM values (positive floats)
        let valid_values = [0.1, 1.0, 45.0, 100.0, 500.0];

        for cfm in valid_values {
            assert!(
                CfmMappingData::validate_cfm(cfm).is_ok(),
                "CFM {} should be valid",
                cfm
            );
        }
    }

    #[test]
    fn test_cfm_validation_zero() {
        // Zero CFM should be invalid (no airflow doesn't make sense as a mapping)
        let result = CfmMappingData::validate_cfm(0.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_cfm_validation_negative() {
        // Negative CFM should be invalid
        let negative_values = [-1.0, -0.1, -100.0];

        for cfm in negative_values {
            assert!(
                CfmMappingData::validate_cfm(cfm).is_err(),
                "CFM {} should be invalid",
                cfm
            );
        }
    }

    #[test]
    fn test_cfm_mapping_data_operations() {
        let mut cfm_data = CfmMappingData::default();

        // Initially empty
        assert!(cfm_data.mappings.is_empty());

        // Add a mapping
        cfm_data.set(0, 45.0);
        assert_eq!(cfm_data.get(0), Some(45.0));

        // Add another mapping
        cfm_data.set(5, 60.0);
        assert_eq!(cfm_data.get(5), Some(60.0));

        // Update existing mapping
        cfm_data.set(0, 50.0);
        assert_eq!(cfm_data.get(0), Some(50.0));

        // Non-existent port returns None
        assert_eq!(cfm_data.get(9), None);
    }

    #[test]
    fn test_cfm_mapping_data_remove() {
        let mut cfm_data = CfmMappingData::default();

        // Add and remove
        cfm_data.set(3, 75.0);
        assert!(cfm_data.get(3).is_some());

        let removed = cfm_data.remove(3);
        assert!(removed);
        assert!(cfm_data.get(3).is_none());

        // Remove non-existent returns false
        let removed_again = cfm_data.remove(3);
        assert!(!removed_again);
    }

    #[test]
    fn test_set_cfm_request_deserialization() {
        use openfan_core::api;

        let json = r#"{"cfm_at_100": 45.0}"#;
        let request: api::SetCfmRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.cfm_at_100, 45.0);
    }

    #[test]
    fn test_set_cfm_request_with_integer() {
        use openfan_core::api;

        // Integer should parse as f64
        let json = r#"{"cfm_at_100": 100}"#;
        let request: api::SetCfmRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.cfm_at_100, 100.0);
    }

    #[test]
    fn test_cfm_response_structures() {
        use openfan_core::api;
        use std::collections::HashMap;

        // CfmGetResponse
        let get_response = api::CfmGetResponse {
            port: 5,
            cfm_at_100: 45.0,
        };
        assert_eq!(get_response.port, 5);
        assert_eq!(get_response.cfm_at_100, 45.0);

        // CfmListResponse
        let mut mappings = HashMap::new();
        mappings.insert(0, 30.0);
        mappings.insert(5, 45.0);
        let list_response = api::CfmListResponse { mappings };
        assert_eq!(list_response.mappings.len(), 2);
        assert_eq!(list_response.mappings.get(&0), Some(&30.0));
        assert_eq!(list_response.mappings.get(&5), Some(&45.0));
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

    /// Test harness that keeps the config directory alive for the duration of the test.
    /// The TempDir must outlive the router since CFM handlers persist config to disk.
    struct TestApp {
        router: Router,
        _config_dir: TempDir,
    }

    impl TestApp {
        async fn new() -> Self {
            let board_info = BoardType::OpenFanStandard.to_board_info();
            let config_dir = tempfile::tempdir().unwrap();

            // Create data directory inside temp dir
            let data_dir = config_dir.path().join("data");
            std::fs::create_dir_all(&data_dir).unwrap();

            // Note: data_dir MUST be at top level in TOML, before any [section]
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
    async fn test_list_cfm() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/default/cfm/list")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        // Should have valid mappings structure
        let data = json.get("data").unwrap();
        assert!(data.get("mappings").is_some());
    }

    #[tokio::test]
    async fn test_get_cfm_valid_port() {
        let app = TestApp::new().await;

        // First set a CFM value
        let _ = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/controller/default/cfm/0")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"cfm_at_100": 45.0}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then get it
        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/default/cfm/0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let data = json.get("data").unwrap();
        assert_eq!(data.get("port").unwrap().as_u64().unwrap(), 0);
    }

    #[tokio::test]
    async fn test_get_cfm_invalid_port() {
        let app = TestApp::new().await;

        // Port 99 is out of range
        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/default/cfm/99")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_set_cfm_valid() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/controller/default/cfm/0")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"cfm_at_100": 45.0}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_set_cfm_zero_invalid() {
        let app = TestApp::new().await;

        // Zero CFM should be rejected
        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/controller/default/cfm/0")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"cfm_at_100": 0.0}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_set_cfm_negative_invalid() {
        let app = TestApp::new().await;

        // Negative CFM should be rejected
        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/controller/default/cfm/0")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"cfm_at_100": -10.0}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_delete_cfm_after_set() {
        let app = TestApp::new().await;

        // First set a CFM value
        let set_response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/controller/default/cfm/5")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"cfm_at_100": 30.0}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(set_response.status(), StatusCode::OK);

        // Then delete it
        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/api/v0/controller/default/cfm/5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}

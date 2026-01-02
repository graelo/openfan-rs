//! Info handlers for system information and root endpoint

use crate::api::error::ApiError;
use crate::api::AppState;

use axum::{extract::State, Json};
use openfan_core::api;
use serde_json::{json, Value};
use tracing::{debug, info, warn};

/// Handle the root endpoint.
///
/// Provide basic service identification and status. Useful for health checks
/// and verifying the API is accessible.
///
/// # Endpoint
///
/// `GET /`
///
/// # Returns
///
/// Return service name, version, and status.
pub(crate) async fn root() -> Result<Json<api::ApiResponse<Value>>, ApiError> {
    debug!("Request: GET /");

    let data = json!({
        "service": "OpenFAN Controller API Server",
        "version": "1.0.0",
        "status": "ok"
    });

    Ok(Json(api::ApiResponse::success(data)))
}

/// Retrieve comprehensive system information.
///
/// Provide details about the server software, hardware connection status,
/// uptime, and hardware/firmware information if available.
///
/// # Endpoint
///
/// `GET /api/v0/info`
///
/// # Returns
///
/// - `version` - Server version
/// - `hardware_connected` - Whether fan controller hardware is connected
/// - `uptime` - Server uptime in seconds
/// - `software` - Software version and build information
/// - `hardware` - Hardware information (if connected, may be None on error)
/// - `firmware` - Firmware version (if connected, may be None on error)
///
/// # Behavior
///
/// - If hardware is not connected, hardware and firmware fields are None
/// - Hardware/firmware queries are logged but failures don't cause errors
pub(crate) async fn get_info(
    State(state): State<AppState>,
) -> Result<Json<api::ApiResponse<api::InfoResponse>>, ApiError> {
    use crate::hardware::ConnectionState;

    debug!("Request: GET /api/v0/info");

    // Calculate actual uptime
    let uptime = state.start_time.elapsed().as_secs();

    // Software information
    let software = "OpenFAN Server v1.0.0\r\nBuild: 2024-10-08".to_string();

    // Get connection state and hardware info via connection manager
    let (
        hardware_connected,
        connection_status,
        reconnect_count,
        reconnection_enabled,
        time_since_disconnect_secs,
        hardware_info,
        firmware_info,
    ) = if let Some(cm) = &state.connection_manager {
        let conn_state = cm.connection_state().await;
        let is_connected = conn_state == ConnectionState::Connected;
        let status = conn_state.as_str().to_string();
        let count = cm.reconnect_count();
        let recon_enabled = cm.reconnection_enabled();
        let time_since = cm
            .time_since_disconnect()
            .await
            .map(|d| d.as_secs());

        // Try to get hardware and firmware info if connected
        let (hw, fw) = if is_connected {
            let result = cm
                .with_controller(|controller| {
                    Box::pin(async move {
                        let hw = match controller.get_hw_info().await {
                            Ok(info) => {
                                debug!("Retrieved hardware info: {}", info);
                                Some(info)
                            }
                            Err(e) => {
                                warn!("Failed to retrieve hardware info: {}", e);
                                None
                            }
                        };

                        let fw = match controller.get_fw_info().await {
                            Ok(info) => {
                                debug!("Retrieved firmware info: {}", info);
                                Some(info)
                            }
                            Err(e) => {
                                warn!("Failed to retrieve firmware info: {}", e);
                                None
                            }
                        };

                        Ok((hw, fw))
                    })
                })
                .await;

            match result {
                Ok((hw, fw)) => (hw, fw),
                Err(e) => {
                    warn!("Failed to get hardware info: {}", e);
                    (None, None)
                }
            }
        } else {
            (None, None)
        };

        (is_connected, status, count, recon_enabled, time_since, hw, fw)
    } else {
        // Mock mode
        (false, "mock".to_string(), 0, false, None, None, None)
    };

    let info_response = api::InfoResponse {
        version: "1.0.0".to_string(),
        board_info: (*state.board_info).clone(),
        hardware_connected,
        connection_status,
        reconnect_count,
        reconnection_enabled,
        time_since_disconnect_secs,
        uptime,
        software,
        hardware: hardware_info,
        firmware: firmware_info,
    };

    Ok(Json(api::ApiResponse::success(info_response)))
}

/// Trigger a manual reconnection attempt.
///
/// Forces the server to attempt reconnecting to the hardware device.
/// Useful when hardware has been physically reconnected and immediate
/// reconnection is desired without waiting for heartbeat detection.
///
/// # Endpoint
///
/// `POST /api/v0/reconnect`
///
/// # Returns
///
/// - Success: Empty response on successful reconnection
/// - Error: 503 if reconnection fails or is in progress
/// - Error: 400 if in mock mode (no hardware to reconnect)
///
/// # Behavior
///
/// - In mock mode, returns an error since there's no hardware to reconnect
/// - Triggers immediate reconnection attempt with exponential backoff
/// - Returns success only after connection is verified
pub(crate) async fn reconnect(
    State(state): State<AppState>,
) -> Result<Json<api::ApiResponse<()>>, ApiError> {
    debug!("Request: POST /api/v0/reconnect");

    let Some(cm) = &state.connection_manager else {
        return Err(ApiError::bad_request(
            "Cannot reconnect in mock mode - no hardware configured".to_string(),
        ));
    };

    info!("Manual reconnection requested");
    cm.force_reconnect().await?;
    info!("Manual reconnection successful");

    Ok(Json(api::ApiResponse::success(())))
}

#[cfg(test)]
mod tests {
    use openfan_core::api;
    use openfan_core::board::BoardType;
    use serde_json::Value;

    #[test]
    fn test_api_response_success() {
        let response: api::ApiResponse<Value> =
            api::ApiResponse::success(serde_json::json!({"test": 1}));
        match response {
            api::ApiResponse::Success { data } => {
                assert_eq!(data["test"], 1);
            }
            api::ApiResponse::Error { .. } => panic!("Expected success response"),
        }
    }

    #[test]
    fn test_api_response_error() {
        let response: api::ApiResponse<()> = api::ApiResponse::error("test error".to_string());
        match response {
            api::ApiResponse::Success { .. } => panic!("Expected error response"),
            api::ApiResponse::Error { error } => {
                assert_eq!(error, "test error");
            }
        }
    }

    #[test]
    fn test_info_response_structure() {
        let board_info = BoardType::OpenFanStandard.to_board_info();

        let info = api::InfoResponse {
            version: "1.0.0".to_string(),
            board_info,
            hardware_connected: true,
            connection_status: "connected".to_string(),
            reconnect_count: 0,
            reconnection_enabled: true,
            time_since_disconnect_secs: None,
            uptime: 3600,
            software: "OpenFAN Server v1.0.0".to_string(),
            hardware: Some("<HW|Model:Standard;Rev:1.0>".to_string()),
            firmware: Some("<FW|Version:1.2.3>".to_string()),
        };

        assert_eq!(info.version, "1.0.0");
        assert!(info.hardware_connected);
        assert_eq!(info.connection_status, "connected");
        assert_eq!(info.uptime, 3600);
        assert!(info.hardware.is_some());
        assert!(info.firmware.is_some());
    }

    #[test]
    fn test_info_response_without_hardware() {
        let board_info = BoardType::OpenFanStandard.to_board_info();

        let info = api::InfoResponse {
            version: "1.0.0".to_string(),
            board_info,
            hardware_connected: false,
            connection_status: "mock".to_string(),
            reconnect_count: 0,
            reconnection_enabled: false,
            time_since_disconnect_secs: None,
            uptime: 120,
            software: "OpenFAN Server v1.0.0".to_string(),
            hardware: None,
            firmware: None,
        };

        assert!(!info.hardware_connected);
        assert_eq!(info.connection_status, "mock");
        assert!(info.hardware.is_none());
        assert!(info.firmware.is_none());
    }

    #[test]
    fn test_info_response_serialization() {
        let board_info = BoardType::OpenFanStandard.to_board_info();

        let info = api::InfoResponse {
            version: "1.0.0".to_string(),
            board_info,
            hardware_connected: true,
            connection_status: "connected".to_string(),
            reconnect_count: 2,
            reconnection_enabled: true,
            time_since_disconnect_secs: Some(30),
            uptime: 60,
            software: "Test".to_string(),
            hardware: None,
            firmware: None,
        };

        // Should serialize without error
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"version\":\"1.0.0\""));
        assert!(json.contains("\"hardware_connected\":true"));
        assert!(json.contains("\"connection_status\":\"connected\""));
        assert!(json.contains("\"reconnect_count\":2"));
        assert!(json.contains("\"uptime\":60"));
    }

    #[test]
    fn test_board_info_in_response() {
        let board_info = BoardType::OpenFanStandard.to_board_info();

        assert_eq!(board_info.name, "OpenFAN Standard");
        assert_eq!(board_info.fan_count, 10);
        assert_eq!(board_info.usb_vid, 0x2E8A);
        assert_eq!(board_info.usb_pid, 0x000A);
    }

    #[test]
    fn test_uptime_calculation() {
        use std::time::{Duration, Instant};

        // Simulate uptime calculation
        let start_time = Instant::now();
        std::thread::sleep(Duration::from_millis(10));
        let uptime_ms = start_time.elapsed().as_millis();

        // Should be at least 10ms
        assert!(uptime_ms >= 10);
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
        let state = AppState::new(board_info, config, None);
        create_router(state)
    }

    async fn body_string(body: Body) -> String {
        let bytes = body.collect().await.unwrap().to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn test_root_endpoint() {
        let app = create_test_app().await;

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        // Verify basic service info
        let data = json.get("data").unwrap();
        assert!(data.get("service").is_some());
        assert!(data.get("status").is_some());
        assert_eq!(data.get("status").unwrap().as_str().unwrap(), "ok");
    }

    #[tokio::test]
    async fn test_get_info_without_hardware() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/info")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        let data = json.get("data").unwrap();

        // Verify structure
        assert!(data.get("version").is_some());
        assert!(data.get("hardware_connected").is_some());
        assert!(data.get("uptime").is_some());
        assert!(data.get("board_info").is_some());

        // Without hardware, hardware_connected should be false
        assert!(!data.get("hardware_connected").unwrap().as_bool().unwrap());

        // Hardware and firmware should be null or absent without hardware
        let hardware = data.get("hardware");
        assert!(hardware.is_none() || hardware.unwrap().is_null());
        let firmware = data.get("firmware");
        assert!(firmware.is_none() || firmware.unwrap().is_null());

        // Uptime should be present and valid (u64 is inherently non-negative)
        assert!(data.get("uptime").unwrap().as_u64().is_some());
    }

    #[tokio::test]
    async fn test_get_info_board_info_structure() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/info")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        let data = json.get("data").unwrap();
        let board_info = data.get("board_info").unwrap();

        // Verify Standard board info
        assert_eq!(
            board_info.get("name").unwrap().as_str().unwrap(),
            "OpenFAN Standard"
        );
        assert_eq!(board_info.get("fan_count").unwrap().as_u64().unwrap(), 10);
        assert_eq!(board_info.get("usb_vid").unwrap().as_u64().unwrap(), 0x2E8A);
        assert_eq!(board_info.get("usb_pid").unwrap().as_u64().unwrap(), 0x000A);
    }
}

//! Info handlers for system information and root endpoint

use crate::api::error::ApiError;
use crate::api::AppState;

use axum::{extract::State, Json};
use openfan_core::api::{ApiResponse, InfoResponse};
use serde_json::{json, Value};
use tracing::{debug, warn};

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
pub(crate) async fn root() -> Result<Json<ApiResponse<Value>>, ApiError> {
    debug!("Request: GET /");

    let data = json!({
        "service": "OpenFAN Controller API Server",
        "version": "1.0.0",
        "status": "ok"
    });

    Ok(Json(ApiResponse::success(data)))
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
) -> Result<Json<ApiResponse<InfoResponse>>, ApiError> {
    debug!("Request: GET /api/v0/info");

    let hardware_connected = state.fan_controller.is_some();

    // Calculate actual uptime
    let uptime = state.start_time.elapsed().as_secs();

    // Software information
    let software = "OpenFAN Server v1.0.0\r\nBuild: 2024-10-08".to_string();

    // Try to get hardware and firmware information if hardware is connected
    let (hardware_info, firmware_info) = if let Some(fan_controller) = &state.fan_controller {
        let mut controller = fan_controller.lock().await;

        let hardware = match controller.get_hw_info().await {
            Ok(hw_info) => {
                debug!("Retrieved hardware info: {}", hw_info);
                Some(hw_info)
            }
            Err(e) => {
                warn!("Failed to retrieve hardware info: {}", e);
                None
            }
        };

        let firmware = match controller.get_fw_info().await {
            Ok(fw_info) => {
                debug!("Retrieved firmware info: {}", fw_info);
                Some(fw_info)
            }
            Err(e) => {
                warn!("Failed to retrieve firmware info: {}", e);
                None
            }
        };

        (hardware, firmware)
    } else {
        (None, None)
    };

    let info_response = InfoResponse {
        version: "1.0.0".to_string(),
        board_info: (*state.board_info).clone(),
        hardware_connected,
        uptime,
        software,
        hardware: hardware_info,
        firmware: firmware_info,
    };

    Ok(Json(ApiResponse::success(info_response)))
}

#[cfg(test)]
mod tests {
    use openfan_core::api::{ApiResponse, InfoResponse};
    use openfan_core::board::BoardType;
    use serde_json::Value;

    #[test]
    fn test_api_response_success() {
        let response: ApiResponse<Value> = ApiResponse::success(serde_json::json!({"test": 1}));
        match response {
            ApiResponse::Success { data } => {
                assert_eq!(data["test"], 1);
            }
            ApiResponse::Error { .. } => panic!("Expected success response"),
        }
    }

    #[test]
    fn test_api_response_error() {
        let response: ApiResponse<()> = ApiResponse::error("test error".to_string());
        match response {
            ApiResponse::Success { .. } => panic!("Expected error response"),
            ApiResponse::Error { error } => {
                assert_eq!(error, "test error");
            }
        }
    }

    #[test]
    fn test_info_response_structure() {
        let board_info = BoardType::OpenFanStandard.to_board_info();

        let info = InfoResponse {
            version: "1.0.0".to_string(),
            board_info,
            hardware_connected: true,
            uptime: 3600,
            software: "OpenFAN Server v1.0.0".to_string(),
            hardware: Some("<HW|Model:Standard;Rev:1.0>".to_string()),
            firmware: Some("<FW|Version:1.2.3>".to_string()),
        };

        assert_eq!(info.version, "1.0.0");
        assert!(info.hardware_connected);
        assert_eq!(info.uptime, 3600);
        assert!(info.hardware.is_some());
        assert!(info.firmware.is_some());
    }

    #[test]
    fn test_info_response_without_hardware() {
        let board_info = BoardType::OpenFanStandard.to_board_info();

        let info = InfoResponse {
            version: "1.0.0".to_string(),
            board_info,
            hardware_connected: false,
            uptime: 120,
            software: "OpenFAN Server v1.0.0".to_string(),
            hardware: None,
            firmware: None,
        };

        assert!(!info.hardware_connected);
        assert!(info.hardware.is_none());
        assert!(info.firmware.is_none());
    }

    #[test]
    fn test_info_response_serialization() {
        let board_info = BoardType::OpenFanStandard.to_board_info();

        let info = InfoResponse {
            version: "1.0.0".to_string(),
            board_info,
            hardware_connected: true,
            uptime: 60,
            software: "Test".to_string(),
            hardware: None,
            firmware: None,
        };

        // Should serialize without error
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"version\":\"1.0.0\""));
        assert!(json.contains("\"hardware_connected\":true"));
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

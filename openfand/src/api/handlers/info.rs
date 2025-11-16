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
pub async fn root() -> Result<Json<ApiResponse<Value>>, ApiError> {
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
pub async fn get_info(
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
        let mut commander = fan_controller.lock().await;

        let hardware = match commander.get_hw_info().await {
            Ok(hw_info) => {
                debug!("Retrieved hardware info: {}", hw_info);
                Some(hw_info)
            }
            Err(e) => {
                warn!("Failed to retrieve hardware info: {}", e);
                None
            }
        };

        let firmware = match commander.get_fw_info().await {
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

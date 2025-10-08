//! Info handlers for system information and root endpoint

use crate::api::error::ApiError;
use crate::api::AppState;

use axum::{extract::State, Json};
use openfan_core::api::{ApiResponse, InfoResponse};
use serde_json::{json, Value};
use tracing::debug;

/// Root endpoint handler
/// GET /
pub async fn root() -> Result<Json<ApiResponse<Value>>, ApiError> {
    debug!("Root handler");

    let data = json!({
        "service": "OpenFAN Controller API Server",
        "version": "1.0.0",
        "status": "ok"
    });

    Ok(Json(ApiResponse::success(data)))
}

/// System information handler
/// GET /api/v0/info
pub async fn get_info(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<InfoResponse>>, ApiError> {
    debug!("Request: GET /api/v0/info");

    let hardware_connected = state.fan_commander.is_some();

    // Calculate actual uptime
    let uptime = state.start_time.elapsed().as_secs();

    // Software information
    let software = "OpenFAN Server v1.0.0\r\nBuild: 2024-10-08".to_string();

    // Try to get hardware and firmware information if hardware is connected
    let (hardware_info, firmware_info) = if let Some(fan_commander) = &state.fan_commander {
        let mut commander = fan_commander.lock().await;

        let hardware = match commander.get_hw_info().await {
            Ok(hw_info) => {
                debug!("Retrieved hardware info: {}", hw_info);
                Some(hw_info)
            }
            Err(e) => {
                debug!("Failed to get hardware info: {}", e);
                None
            }
        };

        let firmware = match commander.get_fw_info().await {
            Ok(fw_info) => {
                debug!("Retrieved firmware info: {}", fw_info);
                Some(fw_info)
            }
            Err(e) => {
                debug!("Failed to get firmware info: {}", e);
                None
            }
        };

        (hardware, firmware)
    } else {
        (None, None)
    };

    let info_response = InfoResponse {
        version: "1.0.0".to_string(),
        hardware_connected,
        uptime,
        software,
        hardware: hardware_info,
        firmware: firmware_info,
    };

    Ok(Json(ApiResponse::success(info_response)))
}

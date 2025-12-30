//! CFM mapping handlers for display-only airflow information

use crate::api::error::ApiError;
use crate::api::AppState;
use crate::{api_fail, api_ok};
use axum::{
    extract::{Path, State},
    Json,
};
use openfan_core::api::{ApiResponse, CfmGetResponse, CfmListResponse, SetCfmRequest};
use openfan_core::config::CfmMappingData;
use tracing::{debug, info};

/// List all CFM mappings.
///
/// Returns a map of port IDs to their CFM@100% values.
///
/// # Endpoint
///
/// `GET /api/v0/cfm/list`
pub(crate) async fn list_cfm(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<CfmListResponse>>, ApiError> {
    debug!("Request: GET /api/v0/cfm/list");

    let cfm_data = state.config.cfm_mappings().await;
    let mappings = cfm_data.mappings.clone();

    let response = CfmListResponse { mappings };

    info!("Listed {} CFM mappings", response.mappings.len());
    api_ok!(response)
}

/// Get CFM mapping for a specific port.
///
/// Returns the CFM@100% value for the specified port.
///
/// # Endpoint
///
/// `GET /api/v0/cfm/:port`
///
/// # Path Parameters
///
/// - `port` - Port identifier (0-9 for standard board)
pub(crate) async fn get_cfm(
    State(state): State<AppState>,
    Path(port): Path<String>,
) -> Result<Json<ApiResponse<CfmGetResponse>>, ApiError> {
    debug!("Request: GET /api/v0/cfm/{}", port);

    // Parse and validate port ID
    let port_id = port
        .parse::<u8>()
        .map_err(|_| ApiError::bad_request(format!("Invalid port ID: {}", port)))?;

    // Validate port ID against board configuration
    state.board_info.validate_fan_id(port_id)?;

    let cfm_data = state.config.cfm_mappings().await;

    match cfm_data.get(port_id) {
        Some(cfm_at_100) => {
            let response = CfmGetResponse {
                port: port_id,
                cfm_at_100,
            };
            debug!("Retrieved CFM mapping for port {}: {}", port_id, cfm_at_100);
            api_ok!(response)
        }
        None => {
            api_fail!(format!("No CFM mapping for port {}", port_id))
        }
    }
}

/// Set CFM mapping for a specific port.
///
/// The CFM@100% value represents the airflow when the fan runs at 100% PWM.
/// Actual CFM is calculated as: `cfm = (pwm / 100.0) * cfm_at_100`
///
/// # Endpoint
///
/// `POST /api/v0/cfm/:port`
///
/// # Path Parameters
///
/// - `port` - Port identifier (0-9 for standard board)
///
/// # Request Body
///
/// ```json
/// {
///     "cfm_at_100": 45.0
/// }
/// ```
pub(crate) async fn set_cfm(
    State(state): State<AppState>,
    Path(port): Path<String>,
    Json(request): Json<SetCfmRequest>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    debug!("Request: POST /api/v0/cfm/{}", port);

    // Parse and validate port ID
    let port_id = port
        .parse::<u8>()
        .map_err(|_| ApiError::bad_request(format!("Invalid port ID: {}", port)))?;

    // Validate port ID against board configuration
    state.board_info.validate_fan_id(port_id)?;

    // Validate CFM value
    if let Err(e) = CfmMappingData::validate_cfm(request.cfm_at_100) {
        return api_fail!(e);
    }

    // Update configuration
    {
        let mut cfm_data = state.config.cfm_mappings_mut().await;
        cfm_data.set(port_id, request.cfm_at_100);
    }

    // Save configuration
    if let Err(e) = state.config.save_cfm_mappings().await {
        return Err(ApiError::internal_error(format!(
            "Failed to save configuration: {}",
            e
        )));
    }

    info!(
        "Set CFM mapping for port {} to {}",
        port_id, request.cfm_at_100
    );
    api_ok!(())
}

/// Delete CFM mapping for a specific port.
///
/// After deletion, no CFM value will be displayed for this port in status output.
///
/// # Endpoint
///
/// `DELETE /api/v0/cfm/:port`
///
/// # Path Parameters
///
/// - `port` - Port identifier (0-9 for standard board)
pub(crate) async fn delete_cfm(
    State(state): State<AppState>,
    Path(port): Path<String>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    debug!("Request: DELETE /api/v0/cfm/{}", port);

    // Parse and validate port ID
    let port_id = port
        .parse::<u8>()
        .map_err(|_| ApiError::bad_request(format!("Invalid port ID: {}", port)))?;

    // Validate port ID against board configuration
    state.board_info.validate_fan_id(port_id)?;

    // Check if mapping exists
    let existed = {
        let mut cfm_data = state.config.cfm_mappings_mut().await;
        cfm_data.remove(port_id)
    };

    if !existed {
        return api_fail!(format!("No CFM mapping for port {}", port_id));
    }

    // Save configuration
    if let Err(e) = state.config.save_cfm_mappings().await {
        return Err(ApiError::internal_error(format!(
            "Failed to save configuration: {}",
            e
        )));
    }

    info!("Deleted CFM mapping for port {}", port_id);
    api_ok!(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_port_id_parsing() {
        assert_eq!("0".parse::<u8>().unwrap(), 0);
        assert_eq!("9".parse::<u8>().unwrap(), 9);
        assert!("abc".parse::<u8>().is_err());
        assert!("-1".parse::<u8>().is_err());
    }
}

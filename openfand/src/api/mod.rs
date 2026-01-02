//! API module for OpenFAN server
//!
//! Contains the REST API implementation with Axum router and handlers.

pub(crate) mod handlers;

use crate::config::RuntimeConfig;
use crate::hardware::ConnectionManager;
use axum::{
    extract::DefaultBodyLimit,
    http::{HeaderValue, Method},
    routing::{get, post},
    Router,
};
use openfan_core::BoardInfo;
use std::sync::Arc;
use std::time::Instant;
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

/// Application state shared across all handlers
#[derive(Clone)]
pub(crate) struct AppState {
    /// Runtime board information
    pub board_info: Arc<BoardInfo>,
    /// Runtime configuration (static config + mutable data)
    pub config: Arc<RuntimeConfig>,
    /// Connection manager for hardware (None in mock mode)
    pub connection_manager: Option<Arc<ConnectionManager>>,
    /// Server start time for uptime calculation
    pub start_time: Instant,
}

impl AppState {
    /// Create new application state
    pub fn new(
        board_info: BoardInfo,
        config: RuntimeConfig,
        connection_manager: Option<Arc<ConnectionManager>>,
    ) -> Self {
        Self {
            board_info: Arc::new(board_info),
            config: Arc::new(config),
            connection_manager,
            start_time: Instant::now(),
        }
    }
}

/// Create the main API router with all endpoints
pub(crate) fn create_router(state: AppState) -> Router {
    info!("Setting up API router...");

    let cors = CorsLayer::new()
        .allow_origin("*".parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(tower_http::cors::Any);

    let middleware_stack = ServiceBuilder::new()
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(DefaultBodyLimit::max(1024 * 1024)); // 1MB limit

    Router::new()
        // Profile endpoints
        .route(
            "/api/v0/profiles/list",
            get(handlers::profiles::list_profiles),
        )
        .route(
            "/api/v0/profiles/add",
            post(handlers::profiles::add_profile),
        )
        .route(
            "/api/v0/profiles/remove",
            get(handlers::profiles::remove_profile),
        )
        .route("/api/v0/profiles/set", get(handlers::profiles::set_profile))
        // Fan status and control endpoints
        .route("/api/v0/fan/status", get(handlers::fans::get_status))
        .route("/api/v0/fan/all/set", get(handlers::fans::set_all_fans))
        .route("/api/v0/fan/{id}/pwm", get(handlers::fans::set_fan_pwm))
        .route("/api/v0/fan/{id}/rpm", get(handlers::fans::set_fan_rpm))
        .route("/api/v0/fan/{id}/rpm/get", get(handlers::fans::get_fan_rpm))
        .route("/api/v0/fan/{id}/set", get(handlers::fans::set_fan_pwm)) // Legacy endpoint
        // Alias endpoints
        .route(
            "/api/v0/alias/all/get",
            get(handlers::aliases::get_all_aliases),
        )
        .route("/api/v0/alias/{id}/get", get(handlers::aliases::get_alias))
        .route("/api/v0/alias/{id}/set", get(handlers::aliases::set_alias))
        .route(
            "/api/v0/alias/{id}",
            axum::routing::delete(handlers::aliases::delete_alias),
        )
        // Zone endpoints
        .route("/api/v0/zones/list", get(handlers::zones::list_zones))
        .route("/api/v0/zones/add", post(handlers::zones::add_zone))
        .route("/api/v0/zone/{name}/get", get(handlers::zones::get_zone))
        .route(
            "/api/v0/zone/{name}/update",
            post(handlers::zones::update_zone),
        )
        .route(
            "/api/v0/zone/{name}/delete",
            get(handlers::zones::delete_zone),
        )
        .route(
            "/api/v0/zone/{name}/apply",
            get(handlers::zones::apply_zone),
        )
        // Thermal curve endpoints
        .route(
            "/api/v0/curves/list",
            get(handlers::thermal_curves::list_curves),
        )
        .route(
            "/api/v0/curves/add",
            post(handlers::thermal_curves::add_curve),
        )
        .route(
            "/api/v0/curve/{name}/get",
            get(handlers::thermal_curves::get_curve),
        )
        .route(
            "/api/v0/curve/{name}/update",
            post(handlers::thermal_curves::update_curve),
        )
        .route(
            "/api/v0/curve/{name}",
            axum::routing::delete(handlers::thermal_curves::delete_curve),
        )
        .route(
            "/api/v0/curve/{name}/interpolate",
            get(handlers::thermal_curves::interpolate_curve),
        )
        // CFM mapping endpoints
        .route("/api/v0/cfm/list", get(handlers::cfm::list_cfm))
        .route("/api/v0/cfm/{port}", get(handlers::cfm::get_cfm))
        .route("/api/v0/cfm/{port}", post(handlers::cfm::set_cfm))
        .route(
            "/api/v0/cfm/{port}",
            axum::routing::delete(handlers::cfm::delete_cfm),
        )
        // System info endpoint
        .route("/api/v0/info", get(handlers::info::get_info))
        // Manual reconnection endpoint
        .route("/api/v0/reconnect", post(handlers::info::reconnect))
        // Root endpoint
        .route("/", get(handlers::info::root))
        .layer(middleware_stack)
        .with_state(state)
}

/// Error handling utilities
pub(crate) mod error {
    use axum::{
        http::StatusCode,
        response::{IntoResponse, Response},
        Json,
    };
    use openfan_core::api::ApiResponse;

    use tracing::error;

    /// Custom error type for API responses
    #[derive(Debug)]
    pub struct ApiError {
        pub status_code: StatusCode,
        pub message: String,
    }

    impl ApiError {
        /// Create a new API error
        pub fn new(status_code: StatusCode, message: impl Into<String>) -> Self {
            Self {
                status_code,
                message: message.into(),
            }
        }

        /// Create a bad request error
        pub fn bad_request(message: impl Into<String>) -> Self {
            Self::new(StatusCode::BAD_REQUEST, message)
        }

        /// Create an internal server error
        pub fn internal_error(message: impl Into<String>) -> Self {
            Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
        }

        /// Create a service unavailable error (for hardware issues)
        pub fn service_unavailable(message: impl Into<String>) -> Self {
            Self::new(StatusCode::SERVICE_UNAVAILABLE, message)
        }
    }

    impl IntoResponse for ApiError {
        fn into_response(self) -> Response {
            error!("API Error {}: {}", self.status_code, self.message);

            let response: ApiResponse<()> = ApiResponse::error(self.message);

            (self.status_code, Json(response)).into_response()
        }
    }

    /// Convert OpenFanError to ApiError
    impl From<openfan_core::OpenFanError> for ApiError {
        fn from(err: openfan_core::OpenFanError) -> Self {
            match err {
                openfan_core::OpenFanError::InvalidInput(msg) => Self::bad_request(msg),
                openfan_core::OpenFanError::InvalidFanId { fan_id, max_fans } => Self::bad_request(
                    format!("Invalid fan ID: {} (must be 0-{})", fan_id, max_fans - 1),
                ),
                openfan_core::OpenFanError::ProfileNotFound(name) => {
                    Self::bad_request(format!("Profile not found: {}", name))
                }
                openfan_core::OpenFanError::ZoneNotFound(name) => {
                    Self::bad_request(format!("Zone not found: {}", name))
                }
                openfan_core::OpenFanError::CurveNotFound(name) => {
                    Self::bad_request(format!("Thermal curve not found: {}", name))
                }
                openfan_core::OpenFanError::CfmMappingNotFound(port) => {
                    Self::bad_request(format!("CFM mapping not found for port {}", port))
                }
                openfan_core::OpenFanError::DeviceNotFound => {
                    Self::service_unavailable("Hardware not available")
                }
                openfan_core::OpenFanError::DeviceDisconnected(msg) => {
                    Self::service_unavailable(format!("Device disconnected: {}", msg))
                }
                openfan_core::OpenFanError::Reconnecting => {
                    Self::service_unavailable("Reconnection in progress, please retry shortly")
                }
                openfan_core::OpenFanError::ReconnectionFailed { attempts, reason } => {
                    Self::service_unavailable(format!(
                        "Reconnection failed after {} attempts: {}",
                        attempts, reason
                    ))
                }
                openfan_core::OpenFanError::Hardware(msg) => Self::service_unavailable(msg),
                openfan_core::OpenFanError::Serial(msg) => Self::service_unavailable(msg),
                openfan_core::OpenFanError::Timeout(msg) => Self::service_unavailable(msg),
                _ => Self::internal_error(err.to_string()),
            }
        }
    }
}

/// Helper macros for common responses
#[macro_export]
macro_rules! api_ok {
    ($data:expr) => {
        Ok(axum::Json(openfan_core::api::ApiResponse::success($data)))
    };
    ($message:expr, $data:expr) => {
        Ok(axum::Json(openfan_core::api::ApiResponse::success($data)))
    };
}

#[macro_export]
macro_rules! api_fail {
    ($message:expr) => {
        Err($crate::api::error::ApiError::bad_request($message))
    };
}

#[cfg(test)]
mod tests {
    use super::error::ApiError;
    use axum::http::StatusCode;
    use openfan_core::OpenFanError;

    #[test]
    fn test_zone_not_found_error_conversion() {
        let error = OpenFanError::ZoneNotFound("test-zone".to_string());
        let api_error: ApiError = error.into();

        assert_eq!(api_error.status_code, StatusCode::BAD_REQUEST);
        assert!(api_error.message.contains("Zone not found"));
        assert!(api_error.message.contains("test-zone"));
    }

    #[test]
    fn test_curve_not_found_error_conversion() {
        let error = OpenFanError::CurveNotFound("test-curve".to_string());
        let api_error: ApiError = error.into();

        assert_eq!(api_error.status_code, StatusCode::BAD_REQUEST);
        assert!(api_error.message.contains("Thermal curve not found"));
        assert!(api_error.message.contains("test-curve"));
    }

    #[test]
    fn test_cfm_mapping_not_found_error_conversion() {
        let error = OpenFanError::CfmMappingNotFound(5);
        let api_error: ApiError = error.into();

        assert_eq!(api_error.status_code, StatusCode::BAD_REQUEST);
        assert!(api_error.message.contains("CFM mapping not found"));
        assert!(api_error.message.contains("5"));
    }

    #[test]
    fn test_profile_not_found_error_conversion() {
        let error = OpenFanError::ProfileNotFound("test-profile".to_string());
        let api_error: ApiError = error.into();

        assert_eq!(api_error.status_code, StatusCode::BAD_REQUEST);
        assert!(api_error.message.contains("Profile not found"));
        assert!(api_error.message.contains("test-profile"));
    }

    #[test]
    fn test_invalid_fan_id_error_conversion() {
        let error = OpenFanError::InvalidFanId {
            fan_id: 15,
            max_fans: 10,
        };
        let api_error: ApiError = error.into();

        assert_eq!(api_error.status_code, StatusCode::BAD_REQUEST);
        assert!(api_error.message.contains("Invalid fan ID"));
        assert!(api_error.message.contains("15"));
        assert!(api_error.message.contains("0-9"));
    }

    #[test]
    fn test_invalid_input_error_conversion() {
        let error = OpenFanError::InvalidInput("bad input".to_string());
        let api_error: ApiError = error.into();

        assert_eq!(api_error.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(api_error.message, "bad input");
    }

    #[test]
    fn test_device_not_found_error_conversion() {
        let error = OpenFanError::DeviceNotFound;
        let api_error: ApiError = error.into();

        assert_eq!(api_error.status_code, StatusCode::SERVICE_UNAVAILABLE);
        assert!(api_error.message.contains("Hardware not available"));
    }

    #[test]
    fn test_hardware_error_conversion() {
        let error = OpenFanError::Hardware("connection failed".to_string());
        let api_error: ApiError = error.into();

        assert_eq!(api_error.status_code, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(api_error.message, "connection failed");
    }

    #[test]
    fn test_serial_error_conversion() {
        let error = OpenFanError::Serial("port busy".to_string());
        let api_error: ApiError = error.into();

        assert_eq!(api_error.status_code, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(api_error.message, "port busy");
    }

    #[test]
    fn test_timeout_error_conversion() {
        let error = OpenFanError::Timeout("operation timed out".to_string());
        let api_error: ApiError = error.into();

        assert_eq!(api_error.status_code, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(api_error.message, "operation timed out");
    }

    #[test]
    fn test_other_error_conversion() {
        let error = OpenFanError::Other("unexpected error".to_string());
        let api_error: ApiError = error.into();

        assert_eq!(api_error.status_code, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(api_error.message.contains("unexpected error"));
    }

    #[test]
    fn test_config_error_falls_through_to_internal() {
        let error = OpenFanError::Config("config issue".to_string());
        let api_error: ApiError = error.into();

        assert_eq!(api_error.status_code, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_device_disconnected_error_conversion() {
        let error = OpenFanError::DeviceDisconnected("USB unplugged".to_string());
        let api_error: ApiError = error.into();

        assert_eq!(api_error.status_code, StatusCode::SERVICE_UNAVAILABLE);
        assert!(api_error.message.contains("Device disconnected"));
        assert!(api_error.message.contains("USB unplugged"));
    }

    #[test]
    fn test_reconnecting_error_conversion() {
        let error = OpenFanError::Reconnecting;
        let api_error: ApiError = error.into();

        assert_eq!(api_error.status_code, StatusCode::SERVICE_UNAVAILABLE);
        assert!(api_error.message.contains("Reconnection in progress"));
    }

    #[test]
    fn test_reconnection_failed_error_conversion() {
        let error = OpenFanError::ReconnectionFailed {
            attempts: 5,
            reason: "Device not found".to_string(),
        };
        let api_error: ApiError = error.into();

        assert_eq!(api_error.status_code, StatusCode::SERVICE_UNAVAILABLE);
        assert!(api_error.message.contains("Reconnection failed"));
        assert!(api_error.message.contains("5 attempts"));
        assert!(api_error.message.contains("Device not found"));
    }
}

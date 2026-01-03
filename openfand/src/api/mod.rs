//! API module for OpenFAN server
//!
//! Contains the REST API implementation with Axum router and handlers.

pub(crate) mod handlers;

use crate::config::RuntimeConfig;
use crate::controllers::{ConnectionManager, ControllerRegistry};
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
    /// Controller registry managing multiple fan controllers
    pub registry: Arc<ControllerRegistry>,
    /// Runtime configuration (static config + global zones)
    pub config: Arc<RuntimeConfig>,
    /// Server start time for uptime calculation
    pub start_time: Instant,

    /// Board info for the default controller (used by system info and zone handlers)
    pub board_info: Arc<BoardInfo>,
    /// Connection manager for the default controller (used by system info and zone handlers)
    pub connection_manager: Option<Arc<ConnectionManager>>,
}

impl AppState {
    /// Create new application state with controller registry
    ///
    /// For multi-controller setups, also provide the default controller's
    /// board_info and connection_manager for backward compatibility.
    ///
    /// # Arguments
    ///
    /// * `registry` - Controller registry managing all controllers
    /// * `config` - Runtime configuration wrapped in Arc for sharing with the
    ///   shutdown handler, which needs access to shutdown settings
    /// * `default_board_info` - Board info for the default/first controller
    /// * `default_connection_manager` - Connection manager for the default/first controller
    pub fn new(
        registry: Arc<ControllerRegistry>,
        config: Arc<RuntimeConfig>,
        default_board_info: BoardInfo,
        default_connection_manager: Option<Arc<ConnectionManager>>,
    ) -> Self {
        Self {
            registry,
            config,
            start_time: Instant::now(),
            board_info: Arc::new(default_board_info),
            connection_manager: default_connection_manager,
        }
    }

    /// Create application state for single-controller mode
    ///
    /// Creates a registry with a single "default" controller.
    /// Used primarily for testing.
    #[cfg(test)]
    pub async fn single_controller(
        board_info: BoardInfo,
        config: Arc<RuntimeConfig>,
        connection_manager: Option<Arc<ConnectionManager>>,
    ) -> Self {
        use crate::controllers::ControllerEntry;

        let registry = ControllerRegistry::new();
        let entry = ControllerEntry::builder("default", board_info.clone())
            .maybe_connection_manager(connection_manager.clone())
            .build();
        registry
            .register(entry)
            .await
            .expect("Failed to register default controller");

        Self {
            registry: Arc::new(registry),
            config,
            start_time: Instant::now(),
            board_info: Arc::new(board_info),
            connection_manager,
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
        // =========================================================================
        // System-wide endpoints
        // =========================================================================
        .route("/api/v0/info", get(handlers::info::get_info))
        .route("/", get(handlers::info::root))
        //
        // =========================================================================
        // Controller management endpoints
        // =========================================================================
        .route(
            "/api/v0/controllers",
            get(handlers::controllers::list_controllers),
        )
        .route(
            "/api/v0/controller/{id}/info",
            get(handlers::controllers::get_controller_info),
        )
        .route(
            "/api/v0/controller/{id}/reconnect",
            post(handlers::controllers::reconnect_controller),
        )
        //
        // =========================================================================
        // Controller-scoped fan endpoints
        // =========================================================================
        .route(
            "/api/v0/controller/{id}/fan/status",
            get(handlers::fans::get_controller_fan_status),
        )
        .route(
            "/api/v0/controller/{id}/fan/all/set",
            get(handlers::fans::set_controller_all_fans),
        )
        .route(
            "/api/v0/controller/{id}/fan/{fan}/pwm",
            get(handlers::fans::set_controller_fan_pwm),
        )
        .route(
            "/api/v0/controller/{id}/fan/{fan}/rpm",
            get(handlers::fans::set_controller_fan_rpm),
        )
        .route(
            "/api/v0/controller/{id}/fan/{fan}/rpm/get",
            get(handlers::fans::get_controller_fan_rpm),
        )
        //
        // =========================================================================
        // Controller-scoped profile endpoints
        // =========================================================================
        .route(
            "/api/v0/controller/{id}/profiles/list",
            get(handlers::profiles::list_controller_profiles),
        )
        .route(
            "/api/v0/controller/{id}/profiles/add",
            post(handlers::profiles::add_controller_profile),
        )
        .route(
            "/api/v0/controller/{id}/profiles/remove",
            get(handlers::profiles::remove_controller_profile),
        )
        .route(
            "/api/v0/controller/{id}/profiles/set",
            get(handlers::profiles::set_controller_profile),
        )
        //
        // =========================================================================
        // Controller-scoped alias endpoints
        // =========================================================================
        .route(
            "/api/v0/controller/{id}/alias/all/get",
            get(handlers::aliases::get_all_controller_aliases),
        )
        .route(
            "/api/v0/controller/{id}/alias/{fan}/get",
            get(handlers::aliases::get_controller_alias),
        )
        .route(
            "/api/v0/controller/{id}/alias/{fan}/set",
            get(handlers::aliases::set_controller_alias),
        )
        .route(
            "/api/v0/controller/{id}/alias/{fan}",
            axum::routing::delete(handlers::aliases::delete_controller_alias),
        )
        //
        // =========================================================================
        // Controller-scoped thermal curve endpoints
        // =========================================================================
        .route(
            "/api/v0/controller/{id}/curves/list",
            get(handlers::thermal_curves::list_controller_curves),
        )
        .route(
            "/api/v0/controller/{id}/curves/add",
            post(handlers::thermal_curves::add_controller_curve),
        )
        .route(
            "/api/v0/controller/{id}/curve/{name}/get",
            get(handlers::thermal_curves::get_controller_curve),
        )
        .route(
            "/api/v0/controller/{id}/curve/{name}/update",
            post(handlers::thermal_curves::update_controller_curve),
        )
        .route(
            "/api/v0/controller/{id}/curve/{name}",
            axum::routing::delete(handlers::thermal_curves::delete_controller_curve),
        )
        .route(
            "/api/v0/controller/{id}/curve/{name}/interpolate",
            get(handlers::thermal_curves::interpolate_controller_curve),
        )
        //
        // =========================================================================
        // Controller-scoped CFM endpoints
        // =========================================================================
        .route(
            "/api/v0/controller/{id}/cfm/list",
            get(handlers::cfm::list_controller_cfm),
        )
        .route(
            "/api/v0/controller/{id}/cfm/{port}",
            get(handlers::cfm::get_controller_cfm),
        )
        .route(
            "/api/v0/controller/{id}/cfm/{port}",
            post(handlers::cfm::set_controller_cfm),
        )
        .route(
            "/api/v0/controller/{id}/cfm/{port}",
            axum::routing::delete(handlers::cfm::delete_controller_cfm),
        )
        //
        // =========================================================================
        // Global zone endpoints (zones span across controllers)
        // =========================================================================
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

    use tracing::debug;

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
            // Log at debug level to avoid duplicating error info already sent to client
            debug!("API Error {}: {}", self.status_code, self.message);

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
                openfan_core::OpenFanError::ControllerNotFound(id) => Self::new(
                    StatusCode::NOT_FOUND,
                    format!("Controller not found: {}", id),
                ),
                openfan_core::OpenFanError::ControllerIdRequired => {
                    Self::bad_request("Controller ID required")
                }
                openfan_core::OpenFanError::DuplicateControllerId(id) => {
                    Self::bad_request(format!("Duplicate controller ID: {}", id))
                }
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

    #[test]
    fn test_controller_not_found_error_conversion() {
        let error = OpenFanError::ControllerNotFound("main".to_string());
        let api_error: ApiError = error.into();

        assert_eq!(api_error.status_code, StatusCode::NOT_FOUND);
        assert!(api_error.message.contains("Controller not found"));
        assert!(api_error.message.contains("main"));
    }

    #[test]
    fn test_controller_id_required_error_conversion() {
        let error = OpenFanError::ControllerIdRequired;
        let api_error: ApiError = error.into();

        assert_eq!(api_error.status_code, StatusCode::BAD_REQUEST);
        assert!(api_error.message.contains("Controller ID required"));
    }

    #[test]
    fn test_duplicate_controller_id_error_conversion() {
        let error = OpenFanError::DuplicateControllerId("gpu".to_string());
        let api_error: ApiError = error.into();

        assert_eq!(api_error.status_code, StatusCode::BAD_REQUEST);
        assert!(api_error.message.contains("Duplicate controller ID"));
        assert!(api_error.message.contains("gpu"));
    }
}

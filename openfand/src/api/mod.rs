//! API module for OpenFAN server
//!
//! Contains the REST API implementation with Axum router and handlers.

pub(crate) mod handlers;

use crate::config::RuntimeConfig;
use crate::hardware::FanController;
use axum::{
    extract::DefaultBodyLimit,
    http::{HeaderValue, Method},
    routing::{get, post},
    Router,
};
use openfan_core::BoardInfo;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
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
    /// Hardware controller
    pub fan_controller: Option<Arc<Mutex<FanController>>>,
    /// Server start time for uptime calculation
    pub start_time: Instant,
}

impl AppState {
    /// Create new application state
    pub fn new(
        board_info: BoardInfo,
        config: RuntimeConfig,
        fan_controller: Option<FanController>,
    ) -> Self {
        Self {
            board_info: Arc::new(board_info),
            config: Arc::new(config),
            fan_controller: fan_controller.map(|fc| Arc::new(tokio::sync::Mutex::new(fc))),
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
        .route("/api/v0/fan/:id/pwm", get(handlers::fans::set_fan_pwm))
        .route("/api/v0/fan/:id/rpm", get(handlers::fans::set_fan_rpm))
        .route("/api/v0/fan/:id/rpm/get", get(handlers::fans::get_fan_rpm))
        .route("/api/v0/fan/:id/set", get(handlers::fans::set_fan_pwm)) // Legacy endpoint
        // Alias endpoints
        .route(
            "/api/v0/alias/all/get",
            get(handlers::aliases::get_all_aliases),
        )
        .route("/api/v0/alias/:id/get", get(handlers::aliases::get_alias))
        .route("/api/v0/alias/:id/set", get(handlers::aliases::set_alias))
        .route(
            "/api/v0/alias/:id",
            axum::routing::delete(handlers::aliases::delete_alias),
        )
        // Zone endpoints
        .route("/api/v0/zones/list", get(handlers::zones::list_zones))
        .route("/api/v0/zones/add", post(handlers::zones::add_zone))
        .route("/api/v0/zone/:name/get", get(handlers::zones::get_zone))
        .route(
            "/api/v0/zone/:name/update",
            post(handlers::zones::update_zone),
        )
        .route(
            "/api/v0/zone/:name/delete",
            get(handlers::zones::delete_zone),
        )
        .route("/api/v0/zone/:name/apply", get(handlers::zones::apply_zone))
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
            "/api/v0/curve/:name/get",
            get(handlers::thermal_curves::get_curve),
        )
        .route(
            "/api/v0/curve/:name/update",
            post(handlers::thermal_curves::update_curve),
        )
        .route(
            "/api/v0/curve/:name",
            axum::routing::delete(handlers::thermal_curves::delete_curve),
        )
        .route(
            "/api/v0/curve/:name/interpolate",
            get(handlers::thermal_curves::interpolate_curve),
        )
        // CFM mapping endpoints
        .route("/api/v0/cfm/list", get(handlers::cfm::list_cfm))
        .route("/api/v0/cfm/:port", get(handlers::cfm::get_cfm))
        .route("/api/v0/cfm/:port", post(handlers::cfm::set_cfm))
        .route(
            "/api/v0/cfm/:port",
            axum::routing::delete(handlers::cfm::delete_cfm),
        )
        // System info endpoint
        .route("/api/v0/info", get(handlers::info::get_info))
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
                openfan_core::OpenFanError::DeviceNotFound => {
                    Self::service_unavailable("Hardware not available")
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

//! API request handlers for the OpenFAN daemon REST API.
//!
//! This module organizes all HTTP endpoint handlers into logical groups:
//!
//! # Handler Modules
//!
//! - [`info`] - System information and root endpoint
//! - [`fans`] - Fan status and control (PWM/RPM)
//! - [`profiles`] - Fan profile management (CRUD operations)
//! - [`aliases`] - Fan alias management
//! - [`zones`] - Zone management for grouped fan control
//! - [`thermal_curves`] - Thermal curve management for temperature-based PWM control
//! - [`cfm`] - CFM mapping management for display-only airflow information
//! - [`controllers`] - Controller management for multi-controller setups
//!
//! # API Structure
//!
//! All handlers follow a consistent pattern:
//! - Accept `State<AppState>` for accessing shared application state
//! - Return `Result<Json<ApiResponse<T>>, ApiError>` for uniform responses
//! - Use `api_ok!()` and `api_fail!()` macros for response construction
//! - Log operations using the `tracing` crate
//!
//! # Multi-Controller Support
//!
//! Controllers are registered in a `ControllerRegistry` and can be accessed
//! via `/api/v0/controllers` and `/api/v0/controller/{id}/*` endpoints.
//!
//! # Mock Mode
//!
//! When hardware is not available (`state.connection_manager` is `None`),
//! handlers return simulated data for testing and development purposes.
//!
//! # Connection Management
//!
//! When hardware is available, the `ConnectionManager` handles automatic
//! reconnection after device disconnects. Handlers use `with_controller()`
//! to execute operations, which automatically detects disconnections and
//! triggers reconnection attempts.

pub(crate) mod aliases;
pub(crate) mod cfm;
pub(crate) mod controllers;
pub(crate) mod fans;
pub(crate) mod info;
pub(crate) mod profiles;
pub(crate) mod thermal_curves;
pub(crate) mod zones;

// Re-export handler functions for easier access
// Re-exports removed - handlers are used directly in routing

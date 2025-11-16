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
//!
//! # API Structure
//!
//! All handlers follow a consistent pattern:
//! - Accept `State<AppState>` for accessing shared application state
//! - Return `Result<Json<ApiResponse<T>>, ApiError>` for uniform responses
//! - Use `api_ok!()` and `api_fail!()` macros for response construction
//! - Log operations using the `tracing` crate
//!
//! # Mock Mode
//!
//! When hardware is not available (`state.fan_controller` is `None`),
//! handlers return simulated data for testing and development purposes.

pub mod aliases;
pub mod fans;
pub mod info;
pub mod profiles;

// Re-export handler functions for easier access
// Re-exports removed - handlers are used directly in routing

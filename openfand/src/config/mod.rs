//! Configuration management module
//!
//! Provides runtime configuration with separate static config and mutable data files.
//!
//! # Architecture
//!
//! Configuration is split into:
//! - Static config (`config.toml`) - Server and hardware settings, loaded once at startup
//! - Global zones (`data_dir/zones.toml`) - Cross-controller zone definitions
//! - Per-controller data (`data_dir/controllers/{id}/`):
//!   - `aliases.toml` - Fan aliases, mutable via API
//!   - `profiles.toml` - Fan profiles, mutable via API
//!   - `thermal_curves.toml` - Thermal response curves
//!   - `cfm_mappings.toml` - CFM calibration data
//!
//! This follows the bind9-style separation where static configuration is kept
//! separate from runtime data that can be modified via API.

mod controller_data;
mod runtime_config;

pub(crate) use controller_data::ControllerData;
pub(crate) use runtime_config::RuntimeConfig;

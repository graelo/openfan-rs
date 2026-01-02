//! Configuration types for OpenFAN
//!
//! This module provides the configuration data structures used by both
//! the daemon (openfand) and potentially other tools.
//!
//! # Architecture
//!
//! Configuration is split into:
//! - [`StaticConfig`] - Server settings, loaded once at startup
//! - [`AliasData`] - Fan aliases, mutable via API
//! - [`ProfileData`] - Fan profiles, mutable via API
//! - [`ZoneData`] - Fan zones for grouped control, mutable via API
//! - [`ThermalCurveData`] - Temperature-to-PWM curves, mutable via API
//! - [`CfmMappingData`] - CFM display mappings, mutable via API
//!
//! Each mutable data type is stored in its own TOML file within the data directory.
//!
//! Hardware is auto-detected via USB VID/PID, `OPENFAN_COMPORT` environment
//! variable, or common device paths (`/dev/ttyUSB0`, `/dev/ttyACM0`, etc.).

mod aliases;
mod cfm_mappings;
mod paths;
mod profiles;
mod static_config;
mod thermal_curves;
mod zones;

pub use aliases::AliasData;
pub use cfm_mappings::CfmMappingData;
pub use paths::{default_config_path, default_data_dir};
pub use profiles::ProfileData;
pub use static_config::{
    ProfileName, ReconnectConfig, ServerConfig, ShutdownConfig, StaticConfig,
    DEFAULT_SAFE_BOOT_PROFILE,
};
pub use thermal_curves::{parse_points, CurvePoint, ThermalCurve, ThermalCurveData};
pub use zones::{Zone, ZoneData};

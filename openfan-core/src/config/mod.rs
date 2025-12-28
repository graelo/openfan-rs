//! Configuration types for OpenFAN
//!
//! This module provides the configuration data structures used by both
//! the daemon (openfand) and potentially other tools.
//!
//! # Architecture
//!
//! Configuration is split into:
//! - [`StaticConfig`] - Server and hardware settings, loaded once at startup
//! - [`AliasData`] - Fan aliases, mutable via API
//! - [`ProfileData`] - Fan profiles, mutable via API
//!
//! Each mutable data type is stored in its own TOML file within the data directory.

mod aliases;
mod paths;
mod profiles;
mod static_config;

pub use aliases::AliasData;
pub use paths::{default_config_path, default_data_dir};
pub use profiles::ProfileData;
pub use static_config::StaticConfig;

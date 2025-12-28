//! Configuration management module
//!
//! Provides runtime configuration with separate static config and mutable data files.
//!
//! # Architecture
//!
//! Configuration is split into:
//! - Static config (`config.toml`) - Server and hardware settings, loaded once at startup
//! - Mutable data files in `data_dir`:
//!   - `aliases.toml` - Fan aliases, mutable via API
//!   - `profiles.toml` - Fan profiles, mutable via API
//!
//! This follows the bind9-style separation where static configuration is kept
//! separate from runtime data that can be modified via API.

mod runtime_config;

pub use runtime_config::RuntimeConfig;

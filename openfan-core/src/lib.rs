//! OpenFAN Core Library
//!
//! Shared types, models, and utilities for the OpenFAN Controller project.
//! This crate is used by both the server and CLI components.

pub mod api;
pub mod config;
pub mod error;
pub mod hardware;
pub mod types;

// Re-export commonly used types
pub use config::{default_config_path, default_data_dir, AliasData, ProfileData, StaticConfig};
pub use error::*;
pub use hardware::*;
pub use types::*;

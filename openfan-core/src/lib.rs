//! OpenFAN Core Library
//!
//! Shared types, models, and utilities for the OpenFAN Controller project.
//! This crate is used by both the server and CLI components.

pub mod api;
pub mod board;
pub mod config;
pub mod error;
pub mod types;

// Re-export commonly used types
pub use board::*;
pub use config::{
    default_config_path, default_data_dir, parse_points, AliasData, CurvePoint, ProfileData,
    ProfileName, ReconnectConfig, ShutdownConfig, StaticConfig, ThermalCurve, ThermalCurveData,
    Zone, ZoneData, DEFAULT_SAFE_BOOT_PROFILE,
};
pub use error::*;
pub use types::*;

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
    AliasData, ControllerConfig, CurvePoint, DEFAULT_SAFE_BOOT_PROFILE, ProfileData, ProfileName,
    ReconnectConfig, ShutdownConfig, StaticConfig, ThermalCurve, ThermalCurveData, Zone, ZoneData,
    ZoneFan, default_config_path, default_data_dir, parse_points,
};
pub use error::*;
pub use types::*;

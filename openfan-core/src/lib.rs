//! OpenFAN Core Library
//!
//! Shared types, models, and utilities for the OpenFAN Controller project.
//! This crate is used by both the server and CLI components.

pub mod api;
pub mod error;
pub mod types;

// Re-export commonly used types
pub use error::*;
pub use types::*;

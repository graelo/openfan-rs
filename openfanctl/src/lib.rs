//! OpenFAN CLI Library
//!
//! This library provides the core functionality for the OpenFAN CLI tool.
//!
//! # Public API
//!
//! The primary public API is the [`client::OpenFanClient`] which provides
//! programmatic access to the OpenFAN server. Configuration types are also
//! available via [`config::CliConfig`] and [`config::ConfigBuilder`].
//!
//! ```no_run
//! use openfanctl::client::OpenFanClient;
//! use std::time::Duration;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let client = OpenFanClient::with_config(
//!     "http://localhost:3000".to_string(),
//!     10,  // timeout in seconds
//!     3,   // max retries
//!     Duration::from_millis(500),  // initial retry delay
//! ).await?;
//!
//! let info = client.get_info().await?;
//! println!("Server version: {}", info.version);
//! # Ok(())
//! # }
//! ```

// Internal CLI implementation - not part of public API
#[doc(hidden)]
pub mod cli;

/// HTTP client for communicating with the OpenFAN server.
pub mod client;

/// Configuration types for the CLI tool.
pub mod config;

// Internal formatting functions - not part of public API
#[doc(hidden)]
pub mod format;

#[cfg(test)]
pub mod test_utils;

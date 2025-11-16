//! Hardware board abstraction layer
//!
//! This module provides a trait-based abstraction for different hardware board configurations.
//! Each board type (OpenFAN v1.0, OpenFAN Mini, etc.) implements the `BoardConfig` trait
//! with its specific hardware characteristics.
//!
//! The abstraction uses const generics and traits to provide:
//! - Zero-cost abstraction (all resolved at compile time)
//! - Type safety (can't mix board configurations)
//! - Extensibility (easy to add new board variants)
//! - Auto-detection (runtime discovery of connected hardware)

use std::marker::PhantomData;

/// Hardware board configuration trait
///
/// Each hardware board variant implements this trait to define its specific
/// characteristics such as fan count, USB identifiers, and communication parameters.
///
/// # Example
///
/// ```
/// use openfan_core::hardware::{BoardConfig, OpenFanV1};
///
/// // Access board properties at compile time
/// const FAN_COUNT: usize = OpenFanV1::FAN_COUNT;
/// const NAME: &str = OpenFanV1::NAME;
/// ```
pub trait BoardConfig: Send + Sync + 'static {
    /// Human-readable board name
    const NAME: &'static str;

    /// Number of fan channels supported by this board
    const FAN_COUNT: usize;

    /// USB Vendor ID for device detection
    const USB_VID: u16;

    /// USB Product ID for device detection
    const USB_PID: u16;

    /// Serial communication baud rate
    const BAUD_RATE: u32;

    /// Default communication timeout in milliseconds
    const DEFAULT_TIMEOUT_MS: u64;

    /// Maximum PWM percentage value (typically 100)
    const MAX_PWM: u32;

    /// Maximum RPM value supported by hardware
    const MAX_RPM: u32;

    /// Minimum operational RPM (below this, fan is turned off)
    const MIN_RPM: u32;
}

/// OpenFAN v1.0 hardware board configuration
///
/// This is the standard 10-fan controller board with the following specifications:
/// - 10 PWM fan channels
/// - USB VID: 0x2E8A (Raspberry Pi Foundation)
/// - USB PID: 0x000A (OpenFAN device)
/// - 115200 baud serial communication
/// - RPM range: 480-16000 (below 480, fan is set to 0/off)
pub struct OpenFanV1;

impl BoardConfig for OpenFanV1 {
    const NAME: &'static str = "OpenFAN v1.0";
    const FAN_COUNT: usize = 10;
    const USB_VID: u16 = 0x2E8A;
    const USB_PID: u16 = 0x000A;
    const BAUD_RATE: u32 = 115200;
    const DEFAULT_TIMEOUT_MS: u64 = 1000;
    const MAX_PWM: u32 = 100;
    const MAX_RPM: u32 = 16000;
    const MIN_RPM: u32 = 480;
}

/// Default board type used throughout the codebase
///
/// Currently set to OpenFanV1. When adding new board support, this can be
/// changed or made configurable via feature flags.
pub type DefaultBoard = OpenFanV1;

/// Backward compatibility: MAX_FANS constant
///
/// This constant is kept for backward compatibility and ease of use.
/// It's now derived from the default board configuration rather than being hardcoded.
pub const MAX_FANS: usize = DefaultBoard::FAN_COUNT;

/// Helper struct for associating a board type at runtime
///
/// This is useful for creating board-aware instances without duplicating
/// the board type information in generic parameters.
#[derive(Debug, Clone, Copy)]
pub struct Board<B: BoardConfig> {
    _marker: PhantomData<B>,
}

impl<B: BoardConfig> Board<B> {
    /// Create a new board marker instance
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }

    /// Get the board name
    pub fn name(&self) -> &'static str {
        B::NAME
    }

    /// Get the fan count
    pub fn fan_count(&self) -> usize {
        B::FAN_COUNT
    }

    /// Validate a fan ID against this board's fan count
    ///
    /// # Errors
    ///
    /// Returns `Err` if the fan ID is out of range (>= FAN_COUNT)
    pub fn validate_fan_id(&self, fan_id: u8) -> Result<(), String> {
        if fan_id as usize >= B::FAN_COUNT {
            Err(format!(
                "Fan ID out of range: {} (must be 0-{})",
                fan_id,
                B::FAN_COUNT - 1
            ))
        } else {
            Ok(())
        }
    }
}

impl<B: BoardConfig> Default for Board<B> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openfan_v1_config() {
        assert_eq!(OpenFanV1::NAME, "OpenFAN v1.0");
        assert_eq!(OpenFanV1::FAN_COUNT, 10);
        assert_eq!(OpenFanV1::USB_VID, 0x2E8A);
        assert_eq!(OpenFanV1::USB_PID, 0x000A);
        assert_eq!(OpenFanV1::BAUD_RATE, 115200);
        assert_eq!(OpenFanV1::DEFAULT_TIMEOUT_MS, 1000);
        assert_eq!(OpenFanV1::MAX_PWM, 100);
        assert_eq!(OpenFanV1::MAX_RPM, 16000);
        assert_eq!(OpenFanV1::MIN_RPM, 480);
    }

    #[test]
    fn test_default_board() {
        assert_eq!(DefaultBoard::FAN_COUNT, 10);
        assert_eq!(MAX_FANS, 10);
    }

    #[test]
    fn test_board_helper() {
        let board = Board::<OpenFanV1>::new();
        assert_eq!(board.name(), "OpenFAN v1.0");
        assert_eq!(board.fan_count(), 10);
    }

    #[test]
    fn test_board_fan_id_validation() {
        let board = Board::<OpenFanV1>::new();

        // Valid fan IDs
        assert!(board.validate_fan_id(0).is_ok());
        assert!(board.validate_fan_id(5).is_ok());
        assert!(board.validate_fan_id(9).is_ok());

        // Invalid fan IDs
        assert!(board.validate_fan_id(10).is_err());
        assert!(board.validate_fan_id(255).is_err());
    }

    #[test]
    fn test_board_default() {
        let board1 = Board::<OpenFanV1>::new();
        let board2 = Board::<OpenFanV1>::default();

        assert_eq!(board1.name(), board2.name());
        assert_eq!(board1.fan_count(), board2.fan_count());
    }
}

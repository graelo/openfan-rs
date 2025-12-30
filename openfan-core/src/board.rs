//! Board definitions and configuration
//!
//! This module provides trait-based abstractions for different hardware board configurations.
//! Each board type (OpenFAN Standard, OpenFAN Micro, etc.) implements the `BoardConfig` trait
//! with its specific characteristics.
//!
//! The abstraction uses const generics and traits to provide:
//! - Zero-cost abstraction (all resolved at compile time)
//! - Type safety (can't mix board configurations)
//! - Extensibility (easy to add new board variants)
//!
//! Note: Actual hardware I/O is in the `openfan-hardware` crate. This module only
//! contains board specifications and type definitions.

use std::marker::PhantomData;

/// Hardware board configuration trait
///
/// Each hardware board variant implements this trait to define its specific
/// characteristics such as fan count, USB identifiers, and communication parameters.
///
/// # Example
///
/// ```
/// use openfan_core::board::{BoardConfig, OpenFanStandard};
///
/// // Access board properties at compile time
/// const FAN_COUNT: usize = OpenFanStandard::FAN_COUNT;
/// const NAME: &str = OpenFanStandard::NAME;
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

/// OpenFAN Standard hardware board configuration
///
/// This is the standard 10-fan controller board with the following specifications:
/// - 10 PWM fan channels
/// - USB VID: 0x2E8A (Raspberry Pi Foundation)
/// - USB PID: 0x000A (OpenFAN device)
/// - 115200 baud serial communication
/// - RPM range: 480-16000 (below 480, fan is set to 0/off)
pub struct OpenFanStandard;

impl BoardConfig for OpenFanStandard {
    const NAME: &'static str = "OpenFAN Standard";
    const FAN_COUNT: usize = 10;
    const USB_VID: u16 = 0x2E8A;
    const USB_PID: u16 = 0x000A;
    const BAUD_RATE: u32 = 115200;
    const DEFAULT_TIMEOUT_MS: u64 = 1000;
    const MAX_PWM: u32 = 100;
    const MAX_RPM: u32 = 16000;
    const MIN_RPM: u32 = 480;
}

/// Runtime board type enumeration
///
/// Unlike the compile-time `BoardConfig` trait, this enum represents
/// board types that can be detected and used at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BoardType {
    /// OpenFAN Standard - Standard 10-fan controller
    OpenFanStandard,
    /// OpenFAN Micro - Compact 4-fan controller (future support)
    OpenFanMicro,
}

impl std::str::FromStr for BoardType {
    type Err = crate::OpenFanError;

    /// Parse board type from string (for CLI --board flag)
    ///
    /// # Examples
    ///
    /// ```
    /// use std::str::FromStr;
    /// use openfan_core::board::BoardType;
    ///
    /// assert!(BoardType::from_str("standard").is_ok());
    /// assert!(BoardType::from_str("micro").is_ok());
    /// assert!(BoardType::from_str("unknown").is_err());
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "standard" | "openfan-standard" => Ok(BoardType::OpenFanStandard),
            "micro" | "openfan-micro" => Ok(BoardType::OpenFanMicro),
            _ => Err(crate::OpenFanError::InvalidInput(format!(
                "Unknown board type: '{}'. Valid options: standard, micro",
                s
            ))),
        }
    }
}

impl BoardType {
    /// Get human-readable board name
    pub fn name(&self) -> &'static str {
        match self {
            BoardType::OpenFanStandard => OpenFanStandard::NAME,
            BoardType::OpenFanMicro => "OpenFAN Micro", // TODO: Add OpenFanMicro BoardConfig
        }
    }

    /// Get fan count for this board type
    pub fn fan_count(&self) -> usize {
        match self {
            BoardType::OpenFanStandard => OpenFanStandard::FAN_COUNT,
            BoardType::OpenFanMicro => 1, // TODO: Use OpenFanMicro::FAN_COUNT
        }
    }

    /// Get USB VID for this board type
    pub fn usb_vid(&self) -> u16 {
        match self {
            BoardType::OpenFanStandard => OpenFanStandard::USB_VID,
            BoardType::OpenFanMicro => 0x2E8A,
        }
    }

    /// Get USB PID for this board type
    pub fn usb_pid(&self) -> u16 {
        match self {
            BoardType::OpenFanStandard => OpenFanStandard::USB_PID,
            BoardType::OpenFanMicro => 0x000B,
        }
    }

    /// Convert to runtime board info
    pub fn to_board_info(self) -> BoardInfo {
        match self {
            BoardType::OpenFanStandard => BoardInfo {
                board_type: BoardType::OpenFanStandard,
                name: OpenFanStandard::NAME.to_string(),
                fan_count: OpenFanStandard::FAN_COUNT,
                usb_vid: OpenFanStandard::USB_VID,
                usb_pid: OpenFanStandard::USB_PID,
                max_pwm: OpenFanStandard::MAX_PWM,
                max_rpm: OpenFanStandard::MAX_RPM,
                min_rpm: OpenFanStandard::MIN_RPM,
                baud_rate: OpenFanStandard::BAUD_RATE,
            },
            BoardType::OpenFanMicro => BoardInfo {
                board_type: BoardType::OpenFanMicro,
                name: "OpenFAN Micro".to_string(),
                fan_count: 1,
                usb_vid: 0x2E8A,
                usb_pid: 0x000B,
                max_pwm: 100,
                max_rpm: 16000,
                min_rpm: 480,
                baud_rate: 115200,
            },
        }
    }
}

/// Runtime board information (non-generic)
///
/// This struct contains all board configuration at runtime without requiring
/// generic type parameters. It's designed to be stored in `Arc<BoardInfo>` and
/// shared across the application.
///
/// Unlike `BoardConfig` which is a compile-time trait, `BoardInfo` provides
/// runtime flexibility for dynamic board detection.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BoardInfo {
    /// Board type variant
    pub board_type: BoardType,
    /// Human-readable board name
    pub name: String,
    /// Number of fan channels supported
    pub fan_count: usize,
    /// USB Vendor ID
    pub usb_vid: u16,
    /// USB Product ID
    pub usb_pid: u16,
    /// Maximum PWM percentage value
    pub max_pwm: u32,
    /// Maximum RPM value
    pub max_rpm: u32,
    /// Minimum operational RPM
    pub min_rpm: u32,
    /// Serial communication baud rate
    pub baud_rate: u32,
}

impl BoardInfo {
    /// Validate a fan ID against this board's fan count
    ///
    /// # Errors
    ///
    /// Returns an error if the fan ID is out of range (>= fan_count)
    ///
    /// # Examples
    ///
    /// ```
    /// use openfan_core::board::BoardType;
    ///
    /// let board = BoardType::OpenFanStandard.to_board_info();
    /// assert!(board.validate_fan_id(0).is_ok());
    /// assert!(board.validate_fan_id(9).is_ok());
    /// assert!(board.validate_fan_id(10).is_err());
    /// ```
    pub fn validate_fan_id(&self, fan_id: u8) -> crate::Result<()> {
        if fan_id as usize >= self.fan_count {
            return Err(crate::OpenFanError::InvalidFanId {
                fan_id,
                max_fans: self.fan_count,
            });
        }
        Ok(())
    }

    /// Validate a PWM value against this board's maximum
    ///
    /// # Errors
    ///
    /// Returns an error if the PWM value exceeds max_pwm
    pub fn validate_pwm(&self, pwm: u32) -> crate::Result<()> {
        if pwm > self.max_pwm {
            return Err(crate::OpenFanError::InvalidInput(format!(
                "PWM must be 0-{}, got {}",
                self.max_pwm, pwm
            )));
        }
        Ok(())
    }

    /// Validate an RPM value against this board's range
    ///
    /// # Errors
    ///
    /// Returns an error if the RPM value exceeds max_rpm
    pub fn validate_rpm(&self, rpm: u32) -> crate::Result<()> {
        if rpm > 0 && rpm < self.min_rpm {
            return Err(crate::OpenFanError::InvalidInput(format!(
                "RPM must be 0 or >= {}, got {}",
                self.min_rpm, rpm
            )));
        }
        if rpm > self.max_rpm {
            return Err(crate::OpenFanError::InvalidInput(format!(
                "RPM must be <= {}, got {}",
                self.max_rpm, rpm
            )));
        }
        Ok(())
    }
}

/// Default board type used throughout the codebase
///
/// Currently set to OpenFanStandard. When adding new board support, this can be
/// changed or made configurable via feature flags.
pub type DefaultBoard = OpenFanStandard;

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
    fn test_openfan_standard_config() {
        assert_eq!(OpenFanStandard::NAME, "OpenFAN Standard");
        assert_eq!(OpenFanStandard::FAN_COUNT, 10);
        assert_eq!(OpenFanStandard::USB_VID, 0x2E8A);
        assert_eq!(OpenFanStandard::USB_PID, 0x000A);
        assert_eq!(OpenFanStandard::BAUD_RATE, 115200);
        assert_eq!(OpenFanStandard::DEFAULT_TIMEOUT_MS, 1000);
        assert_eq!(OpenFanStandard::MAX_PWM, 100);
        assert_eq!(OpenFanStandard::MAX_RPM, 16000);
        assert_eq!(OpenFanStandard::MIN_RPM, 480);
    }

    #[test]
    fn test_default_board() {
        assert_eq!(DefaultBoard::FAN_COUNT, 10);
        assert_eq!(MAX_FANS, 10);
    }

    #[test]
    fn test_board_helper() {
        let board = Board::<OpenFanStandard>::new();
        assert_eq!(board.name(), "OpenFAN Standard");
        assert_eq!(board.fan_count(), 10);
    }

    #[test]
    fn test_board_fan_id_validation() {
        let board = Board::<OpenFanStandard>::new();

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
        let board1 = Board::<OpenFanStandard>::new();
        let board2 = Board::<OpenFanStandard>::default();

        assert_eq!(board1.name(), board2.name());
        assert_eq!(board1.fan_count(), board2.fan_count());
    }
}

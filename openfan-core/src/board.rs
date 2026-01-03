//! Board definitions and configuration
//!
//! This module provides trait-based abstractions for different hardware board configurations.
//! Each board type (OpenFAN Standard, Custom, etc.) implements the `BoardConfig` trait
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
/// characteristics such as fan count and communication parameters.
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

    /// Serial communication baud rate
    const BAUD_RATE: u32;

    /// Default communication timeout in milliseconds
    const DEFAULT_TIMEOUT_MS: u64;

    /// Maximum PWM percentage value (typically 100)
    const MAX_PWM: u32;

    /// Minimum RPM for target mode (per OpenFAN docs: 500)
    const MIN_TARGET_RPM: u32;

    /// Maximum RPM for target mode (per OpenFAN docs: 9000)
    const MAX_TARGET_RPM: u32;
}

/// OpenFAN Standard hardware board configuration
///
/// This is the standard 10-fan controller board with the following specifications:
/// - 10 PWM fan channels
/// - 115200 baud serial communication
/// - PWM mode: 0-100%
/// - RPM target mode: 500-9000 (per OpenFAN docs)
pub struct OpenFanStandard;

impl BoardConfig for OpenFanStandard {
    const NAME: &'static str = "OpenFAN Standard";
    const FAN_COUNT: usize = 10;
    const BAUD_RATE: u32 = 115200;
    const DEFAULT_TIMEOUT_MS: u64 = 1000;
    const MAX_PWM: u32 = 100;
    const MIN_TARGET_RPM: u32 = 500;
    const MAX_TARGET_RPM: u32 = 9000;
}

/// Runtime board type enumeration
///
/// Unlike the compile-time `BoardConfig` trait, this enum represents
/// board types that can be used at runtime.
///
/// # Adding New Board Types
///
/// To add support for a new board:
/// 1. Add a new variant to this enum with the fan count
/// 2. Implement the `BoardConfig` trait for compile-time constants (optional)
/// 3. Update `FromStr`, `name()`, `fan_count()`, and `to_board_info()`
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum BoardType {
    /// OpenFAN Standard - 10-fan controller
    OpenFanStandard,
    /// Custom/DIY board with configurable fan count
    ///
    /// Use this for custom or modified boards that use serial communication.
    /// The fan count must be specified when creating a Custom board.
    Custom {
        /// Number of fan channels on this custom board (1-16)
        fan_count: usize,
    },
}

impl TryFrom<String> for BoardType {
    type Error = crate::OpenFanError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<BoardType> for String {
    fn from(board_type: BoardType) -> Self {
        match board_type {
            BoardType::OpenFanStandard => "standard".to_string(),
            BoardType::Custom { fan_count } => format!("custom:{}", fan_count),
        }
    }
}

impl std::str::FromStr for BoardType {
    type Err = crate::OpenFanError;

    /// Parse board type from string (for CLI --board flag)
    ///
    /// For custom boards, use "custom:N" where N is the fan count (1-16).
    ///
    /// # Examples
    ///
    /// ```
    /// use std::str::FromStr;
    /// use openfan_core::board::BoardType;
    ///
    /// assert!(BoardType::from_str("standard").is_ok());
    /// assert!(BoardType::from_str("custom:4").is_ok());
    /// assert!(BoardType::from_str("unknown").is_err());
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s_lower = s.to_lowercase();

        // Check for custom:N format
        if let Some(count_str) = s_lower.strip_prefix("custom:") {
            let fan_count: usize = count_str.parse().map_err(|_| {
                crate::OpenFanError::InvalidInput(format!(
                    "Invalid fan count in '{}'. Use 'custom:N' where N is 1-16",
                    s
                ))
            })?;

            if fan_count == 0 || fan_count > 16 {
                return Err(crate::OpenFanError::InvalidInput(format!(
                    "Fan count must be 1-16, got {}",
                    fan_count
                )));
            }

            return Ok(BoardType::Custom { fan_count });
        }

        match s_lower.as_str() {
            "standard" | "openfan-standard" => Ok(BoardType::OpenFanStandard),
            "custom" => Err(crate::OpenFanError::InvalidInput(
                "Custom board requires fan count. Use 'custom:N' where N is 1-16".to_string(),
            )),
            _ => Err(crate::OpenFanError::InvalidInput(format!(
                "Unknown board type: '{}'. Valid options: standard, custom:N (where N is fan count 1-16)",
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
            BoardType::Custom { .. } => "Custom Board",
        }
    }

    /// Get fan count for this board type
    pub fn fan_count(&self) -> usize {
        match self {
            BoardType::OpenFanStandard => OpenFanStandard::FAN_COUNT,
            BoardType::Custom { fan_count } => *fan_count,
        }
    }

    /// Convert to runtime board info
    pub fn to_board_info(self) -> BoardInfo {
        match self {
            BoardType::OpenFanStandard => BoardInfo {
                board_type: BoardType::OpenFanStandard,
                name: OpenFanStandard::NAME.to_string(),
                fan_count: OpenFanStandard::FAN_COUNT,
                max_pwm: OpenFanStandard::MAX_PWM,
                min_target_rpm: OpenFanStandard::MIN_TARGET_RPM,
                max_target_rpm: OpenFanStandard::MAX_TARGET_RPM,
                baud_rate: OpenFanStandard::BAUD_RATE,
            },
            BoardType::Custom { fan_count } => BoardInfo {
                board_type: BoardType::Custom { fan_count },
                name: format!("Custom Board ({} fans)", fan_count),
                fan_count,
                max_pwm: 100,
                min_target_rpm: 500,
                max_target_rpm: 9000,
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
/// runtime flexibility for board configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BoardInfo {
    /// Board type variant
    pub board_type: BoardType,
    /// Human-readable board name
    pub name: String,
    /// Number of fan channels supported
    pub fan_count: usize,
    /// Maximum PWM percentage value (0-100)
    pub max_pwm: u32,
    /// Minimum RPM for target mode
    pub min_target_rpm: u32,
    /// Maximum RPM for target mode
    pub max_target_rpm: u32,
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

    /// Validate an RPM value for target mode against this board's range
    ///
    /// # Errors
    ///
    /// Returns an error if the RPM value is outside the valid target range
    pub fn validate_target_rpm(&self, rpm: u32) -> crate::Result<()> {
        if rpm < self.min_target_rpm {
            return Err(crate::OpenFanError::InvalidInput(format!(
                "Target RPM must be >= {}, got {}",
                self.min_target_rpm, rpm
            )));
        }
        if rpm > self.max_target_rpm {
            return Err(crate::OpenFanError::InvalidInput(format!(
                "Target RPM must be <= {}, got {}",
                self.max_target_rpm, rpm
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
    use std::str::FromStr;

    #[test]
    fn test_openfan_standard_config() {
        assert_eq!(OpenFanStandard::NAME, "OpenFAN Standard");
        assert_eq!(OpenFanStandard::FAN_COUNT, 10);
        assert_eq!(OpenFanStandard::BAUD_RATE, 115200);
        assert_eq!(OpenFanStandard::DEFAULT_TIMEOUT_MS, 1000);
        assert_eq!(OpenFanStandard::MAX_PWM, 100);
        assert_eq!(OpenFanStandard::MIN_TARGET_RPM, 500);
        assert_eq!(OpenFanStandard::MAX_TARGET_RPM, 9000);
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

    #[test]
    fn test_board_type_from_str_standard() {
        assert!(matches!(
            BoardType::from_str("standard"),
            Ok(BoardType::OpenFanStandard)
        ));
        assert!(matches!(
            BoardType::from_str("openfan-standard"),
            Ok(BoardType::OpenFanStandard)
        ));
        assert!(matches!(
            BoardType::from_str("STANDARD"),
            Ok(BoardType::OpenFanStandard)
        ));
    }

    #[test]
    fn test_board_type_from_str_custom() {
        let custom = BoardType::from_str("custom:4").unwrap();
        assert!(matches!(custom, BoardType::Custom { fan_count: 4 }));
        assert_eq!(custom.fan_count(), 4);

        let custom_1 = BoardType::from_str("custom:1").unwrap();
        assert_eq!(custom_1.fan_count(), 1);

        let custom_16 = BoardType::from_str("custom:16").unwrap();
        assert_eq!(custom_16.fan_count(), 16);
    }

    #[test]
    fn test_board_type_from_str_custom_invalid() {
        // Missing fan count
        assert!(BoardType::from_str("custom").is_err());

        // Zero fans
        assert!(BoardType::from_str("custom:0").is_err());

        // Too many fans
        assert!(BoardType::from_str("custom:17").is_err());

        // Invalid number
        assert!(BoardType::from_str("custom:abc").is_err());
    }

    #[test]
    fn test_board_type_from_str_unknown() {
        assert!(BoardType::from_str("unknown").is_err());
        assert!(BoardType::from_str("micro").is_err()); // Micro is no longer supported
    }

    #[test]
    fn test_board_type_methods() {
        let standard = BoardType::OpenFanStandard;
        assert_eq!(standard.name(), "OpenFAN Standard");
        assert_eq!(standard.fan_count(), 10);

        let custom = BoardType::Custom { fan_count: 4 };
        assert_eq!(custom.name(), "Custom Board");
        assert_eq!(custom.fan_count(), 4);
    }

    #[test]
    fn test_board_type_to_board_info() {
        let standard_info = BoardType::OpenFanStandard.to_board_info();
        assert_eq!(standard_info.name, "OpenFAN Standard");
        assert_eq!(standard_info.fan_count, 10);
        assert_eq!(standard_info.max_pwm, 100);
        assert_eq!(standard_info.baud_rate, 115200);

        let custom_info = BoardType::Custom { fan_count: 4 }.to_board_info();
        assert_eq!(custom_info.name, "Custom Board (4 fans)");
        assert_eq!(custom_info.fan_count, 4);
        assert_eq!(custom_info.max_pwm, 100);
        assert_eq!(custom_info.baud_rate, 115200);
    }

    #[test]
    fn test_custom_board_info_validation() {
        let info = BoardType::Custom { fan_count: 4 }.to_board_info();

        // Valid fan IDs for 4-fan board
        assert!(info.validate_fan_id(0).is_ok());
        assert!(info.validate_fan_id(3).is_ok());

        // Invalid fan ID
        assert!(info.validate_fan_id(4).is_err());
    }

    #[test]
    fn test_board_info_validate_pwm() {
        let info = BoardType::OpenFanStandard.to_board_info();

        // Valid PWM values
        assert!(info.validate_pwm(0).is_ok());
        assert!(info.validate_pwm(50).is_ok());
        assert!(info.validate_pwm(100).is_ok());

        // Invalid PWM value
        assert!(info.validate_pwm(101).is_err());
    }

    #[test]
    fn test_board_info_validate_target_rpm() {
        let info = BoardType::OpenFanStandard.to_board_info();

        // Valid RPM values
        assert!(info.validate_target_rpm(500).is_ok());
        assert!(info.validate_target_rpm(5000).is_ok());
        assert!(info.validate_target_rpm(9000).is_ok());

        // Invalid RPM values (too low / too high)
        assert!(info.validate_target_rpm(499).is_err());
        assert!(info.validate_target_rpm(9001).is_err());
    }
}

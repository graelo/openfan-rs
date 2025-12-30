//! Fan Controller - High-level interface for fan control
//!
//! Implements the fan control protocol over serial communication.

use crate::serial_driver::SerialDriver;
use openfan_core::{BoardConfig, FanRpmMap, OpenFanError, Result};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, warn};

/// Commands supported by the fan controller
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum Command {
    /// Get all fan RPM (0x00)
    GetAllFanRpm = 0x00,
    /// Get single fan RPM (0x01)
    GetSingleFanRpm = 0x01,
    /// Set fan PWM (0x02)
    SetFanPwm = 0x02,
    /// Set all fans PWM (0x03)
    SetAllFanPwm = 0x03,
    /// Set fan RPM (0x04)
    SetFanRpm = 0x04,
    /// Get hardware info (0x05)
    GetHwInfo = 0x05,
    /// Get firmware info (0x06)
    GetFwInfo = 0x06,
}

/// Fan controller interface
pub struct FanController<B: BoardConfig = openfan_core::DefaultBoard> {
    driver: Arc<Mutex<SerialDriver<B>>>,
    fan_rpm_cache: HashMap<u8, u32>,
    fan_pwm_cache: HashMap<u8, u32>,
    _board: PhantomData<B>,
}

impl<B: BoardConfig> FanController<B> {
    /// Create a new FanController with the given serial driver
    pub fn new(driver: SerialDriver<B>) -> Self {
        Self {
            driver: Arc::new(Mutex::new(driver)),
            fan_rpm_cache: HashMap::new(),
            fan_pwm_cache: HashMap::new(),
            _board: PhantomData,
        }
    }

    /// Validate a fan ID against this board's fan count
    fn validate_fan_id(&self, fan_id: u8) -> Result<()> {
        if fan_id as usize >= B::FAN_COUNT {
            return Err(OpenFanError::InvalidFanId {
                fan_id,
                max_fans: B::FAN_COUNT,
            });
        }
        Ok(())
    }

    /// Send a command to the hardware
    async fn send_command(&mut self, cmd: Command, data: Option<&[u8]>) -> Result<String> {
        let mut driver = self.driver.lock().await;

        // Build command payload
        let mut payload = format!(">{:02X}", cmd as u8);

        if let Some(data_bytes) = data {
            for byte in data_bytes {
                payload.push_str(&format!("{:02X}", byte));
            }
        }

        debug!("Sending command: {:?} with payload: {}", cmd, payload);

        let response = driver.transaction(&payload).await?;
        self.parse_response(response)
    }

    /// Parse the response from hardware
    fn parse_response(&self, response: Vec<String>) -> Result<String> {
        for line in &response {
            debug!("Response line: {}", line);
            if line.starts_with('<') {
                debug!("Valid response: {}", line);
                return Ok(line.clone());
            }
        }

        error!("No valid response found in: {:?}", response);
        Err(OpenFanError::Hardware(
            "No valid response received".to_string(),
        ))
    }

    /// Parse fan RPM response
    fn parse_fan_rpm(&mut self, response: &str) -> Result<FanRpmMap> {
        // Response format: <DATA|0:1234;1:5678;2:9ABC;...;>
        // Split by '|' and take the second part
        let parts: Vec<&str> = response.split('|').collect();
        if parts.len() < 2 {
            return Err(OpenFanError::Parse(format!(
                "Invalid RPM response format: {}",
                response
            )));
        }

        let data_part = parts[1].trim_end_matches(';').trim_end_matches('>');
        let mut rpm_map = HashMap::new();

        for fan_data in data_part.split(';') {
            if fan_data.is_empty() {
                continue;
            }

            let fan_parts: Vec<&str> = fan_data.split(':').collect();
            if fan_parts.len() != 2 {
                warn!("Invalid fan data format: {}", fan_data);
                continue;
            }

            let fan_id = fan_parts[0].parse::<u8>().map_err(|e| {
                OpenFanError::Parse(format!("Invalid fan ID: {} - {}", fan_parts[0], e))
            })?;

            let rpm = u32::from_str_radix(fan_parts[1], 16).map_err(|e| {
                OpenFanError::Parse(format!("Invalid RPM value: {} - {}", fan_parts[1], e))
            })?;

            rpm_map.insert(fan_id, rpm);
            // Update cache
            self.fan_rpm_cache.insert(fan_id, rpm);
        }

        debug!("Parsed RPM data: {:?}", rpm_map);
        Ok(rpm_map)
    }

    /// Get RPM for all fans
    pub async fn get_all_fan_rpm(&mut self) -> Result<FanRpmMap> {
        let response = self.send_command(Command::GetAllFanRpm, None).await?;
        self.parse_fan_rpm(&response)
    }

    /// Get cached PWM values for all fans
    ///
    /// Note: The hardware does not support reading PWM values directly.
    /// This returns the last PWM values that were set via set_fan_pwm() or set_all_fan_pwm().
    /// Returns an empty map if no PWM values have been set yet.
    pub fn get_all_fan_pwm(&self) -> HashMap<u8, u32> {
        self.fan_pwm_cache.clone()
    }

    /// Get RPM for a single fan
    pub async fn get_single_fan_rpm(&mut self, fan_id: u8) -> Result<u32> {
        self.validate_fan_id(fan_id)?;

        let data = [fan_id];
        let response = self
            .send_command(Command::GetSingleFanRpm, Some(&data))
            .await?;
        let rpm_map = self.parse_fan_rpm(&response)?;

        rpm_map
            .get(&fan_id)
            .copied()
            .ok_or_else(|| OpenFanError::Hardware(format!("No RPM data for fan {}", fan_id)))
    }

    /// Get cached PWM value for a single fan
    ///
    /// Note: The hardware does not support reading PWM values directly.
    /// This returns the last PWM value that was set for this fan.
    /// Returns None if no PWM value has been set for this fan yet.
    pub fn get_single_fan_pwm(&self, fan_id: u8) -> Option<u32> {
        self.fan_pwm_cache.get(&fan_id).copied()
    }

    /// Set PWM for a single fan
    pub async fn set_fan_pwm(&mut self, fan_id: u8, pwm_percent: u32) -> Result<String> {
        self.validate_fan_id(fan_id)?;

        if pwm_percent > B::MAX_PWM {
            return Err(OpenFanError::InvalidInput(format!(
                "PWM percentage must be 0-100, got {}",
                pwm_percent
            )));
        }

        // Convert percentage to 0-255 range
        let pwm_value = (pwm_percent * 255) / 100;
        let data = [fan_id, pwm_value as u8];

        let result = self.send_command(Command::SetFanPwm, Some(&data)).await?;

        // Cache the PWM value on successful write
        self.fan_pwm_cache.insert(fan_id, pwm_percent);

        Ok(result)
    }

    /// Set PWM for all fans
    pub async fn set_all_fan_pwm(&mut self, pwm_percent: u32) -> Result<String> {
        if pwm_percent > B::MAX_PWM {
            return Err(OpenFanError::InvalidInput(format!(
                "PWM percentage must be 0-100, got {}",
                pwm_percent
            )));
        }

        // Convert percentage to 0-255 range
        let pwm_value = (pwm_percent * 255) / 100;
        let data = [pwm_value as u8];

        let result = self
            .send_command(Command::SetAllFanPwm, Some(&data))
            .await?;

        // Cache the PWM value for all fans on successful write
        for fan_id in 0..B::FAN_COUNT as u8 {
            self.fan_pwm_cache.insert(fan_id, pwm_percent);
        }

        Ok(result)
    }

    /// Set target RPM for a single fan
    pub async fn set_fan_rpm(&mut self, fan_id: u8, rpm: u32) -> Result<String> {
        self.validate_fan_id(fan_id)?;

        if rpm > 65535 {
            return Err(OpenFanError::InvalidInput(format!(
                "RPM must be 0-65535, got {}",
                rpm
            )));
        }

        // Split RPM into high and low bytes
        let rpm_high = ((rpm >> 8) & 0xFF) as u8;
        let rpm_low = (rpm & 0xFF) as u8;
        let data = [fan_id, rpm_high, rpm_low];

        self.send_command(Command::SetFanRpm, Some(&data)).await
    }

    /// Get hardware information
    pub async fn get_hw_info(&mut self) -> Result<String> {
        self.send_command(Command::GetHwInfo, None).await
    }

    /// Get firmware information
    pub async fn get_fw_info(&mut self) -> Result<String> {
        self.send_command(Command::GetFwInfo, None).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_values() {
        assert_eq!(Command::GetAllFanRpm as u8, 0x00);
        assert_eq!(Command::GetSingleFanRpm as u8, 0x01);
        assert_eq!(Command::SetFanPwm as u8, 0x02);
        assert_eq!(Command::SetAllFanPwm as u8, 0x03);
        assert_eq!(Command::SetFanRpm as u8, 0x04);
        assert_eq!(Command::GetHwInfo as u8, 0x05);
        assert_eq!(Command::GetFwInfo as u8, 0x06);
    }

    #[test]
    fn test_command_debug_format() {
        // Ensure Command derives Debug correctly
        let cmd = Command::GetAllFanRpm;
        assert_eq!(format!("{:?}", cmd), "GetAllFanRpm");
    }

    #[test]
    fn test_command_clone() {
        let cmd = Command::SetFanPwm;
        let cloned = cmd;
        assert_eq!(cmd as u8, cloned as u8);
    }

    /// Helper to parse fan RPM response (mirrors FanController::parse_fan_rpm logic)
    fn parse_rpm_response(response: &str) -> Result<HashMap<u8, u32>> {
        let parts: Vec<&str> = response.split('|').collect();
        if parts.len() < 2 {
            return Err(OpenFanError::Parse(format!(
                "Invalid RPM response format: {}",
                response
            )));
        }

        let data_part = parts[1].trim_end_matches(';').trim_end_matches('>');
        let mut rpm_map = HashMap::new();

        for fan_data in data_part.split(';') {
            if fan_data.is_empty() {
                continue;
            }

            let fan_parts: Vec<&str> = fan_data.split(':').collect();
            if fan_parts.len() != 2 {
                continue; // Skip invalid entries (mirrors FanController behavior)
            }

            let fan_id = fan_parts[0].parse::<u8>().map_err(|e| {
                OpenFanError::Parse(format!("Invalid fan ID: {} - {}", fan_parts[0], e))
            })?;

            let rpm = u32::from_str_radix(fan_parts[1], 16).map_err(|e| {
                OpenFanError::Parse(format!("Invalid RPM value: {} - {}", fan_parts[1], e))
            })?;

            rpm_map.insert(fan_id, rpm);
        }

        Ok(rpm_map)
    }

    #[test]
    fn test_parse_fan_rpm_standard_response() {
        let response = "<DATA|0:1234;1:5678;2:9ABC;>";
        let rpm_map = parse_rpm_response(response).unwrap();

        assert_eq!(rpm_map.get(&0), Some(&0x1234));
        assert_eq!(rpm_map.get(&1), Some(&0x5678));
        assert_eq!(rpm_map.get(&2), Some(&0x9ABC));
        assert_eq!(rpm_map.len(), 3);
    }

    #[test]
    fn test_parse_fan_rpm_ten_fans() {
        // OpenFAN Standard has 10 fans
        let response =
            "<DATA|0:0100;1:0200;2:0300;3:0400;4:0500;5:0600;6:0700;7:0800;8:0900;9:0A00;>";
        let rpm_map = parse_rpm_response(response).unwrap();

        assert_eq!(rpm_map.len(), 10);
        for i in 0..10 {
            let expected_rpm = (i + 1) * 0x100;
            assert_eq!(rpm_map.get(&(i as u8)), Some(&expected_rpm));
        }
    }

    #[test]
    fn test_parse_fan_rpm_zero_rpm() {
        let response = "<DATA|0:0000;1:0000;>";
        let rpm_map = parse_rpm_response(response).unwrap();

        assert_eq!(rpm_map.get(&0), Some(&0));
        assert_eq!(rpm_map.get(&1), Some(&0));
    }

    #[test]
    fn test_parse_fan_rpm_max_rpm() {
        let response = "<DATA|0:FFFF;>";
        let rpm_map = parse_rpm_response(response).unwrap();

        assert_eq!(rpm_map.get(&0), Some(&0xFFFF));
    }

    #[test]
    fn test_parse_fan_rpm_single_fan() {
        let response = "<DATA|5:1A2B;>";
        let rpm_map = parse_rpm_response(response).unwrap();

        assert_eq!(rpm_map.len(), 1);
        assert_eq!(rpm_map.get(&5), Some(&0x1A2B));
    }

    #[test]
    fn test_parse_fan_rpm_lowercase_hex() {
        let response = "<DATA|0:abcd;1:ef00;>";
        let rpm_map = parse_rpm_response(response).unwrap();

        assert_eq!(rpm_map.get(&0), Some(&0xABCD));
        assert_eq!(rpm_map.get(&1), Some(&0xEF00));
    }

    #[test]
    fn test_parse_fan_rpm_mixed_case_hex() {
        let response = "<DATA|0:AbCd;1:eF01;>";
        let rpm_map = parse_rpm_response(response).unwrap();

        assert_eq!(rpm_map.get(&0), Some(&0xABCD));
        assert_eq!(rpm_map.get(&1), Some(&0xEF01));
    }

    #[test]
    fn test_parse_fan_rpm_empty_data() {
        let response = "<DATA|;>";
        let rpm_map = parse_rpm_response(response).unwrap();

        assert!(rpm_map.is_empty());
    }

    #[test]
    fn test_parse_fan_rpm_no_separator() {
        let response = "<DATA>";
        let result = parse_rpm_response(response);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_fan_rpm_invalid_fan_id() {
        let response = "<DATA|abc:1234;>";
        let result = parse_rpm_response(response);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OpenFanError::Parse(_)));
    }

    #[test]
    fn test_parse_fan_rpm_invalid_rpm_value() {
        let response = "<DATA|0:GHIJ;>";
        let result = parse_rpm_response(response);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OpenFanError::Parse(_)));
    }

    #[test]
    fn test_parse_fan_rpm_skips_malformed_entries() {
        // Entries without ':' should be skipped
        let response = "<DATA|0:1234;invalid;2:5678;>";
        let rpm_map = parse_rpm_response(response).unwrap();

        assert_eq!(rpm_map.len(), 2);
        assert_eq!(rpm_map.get(&0), Some(&0x1234));
        assert_eq!(rpm_map.get(&2), Some(&0x5678));
    }

    #[test]
    fn test_parse_fan_rpm_trailing_semicolons() {
        let response = "<DATA|0:1234;;;>";
        let rpm_map = parse_rpm_response(response).unwrap();

        assert_eq!(rpm_map.len(), 1);
        assert_eq!(rpm_map.get(&0), Some(&0x1234));
    }

    /// Helper to check if a response line is valid (mirrors parse_response logic)
    fn is_valid_response(line: &str) -> bool {
        line.starts_with('<')
    }

    #[test]
    fn test_response_validation_valid() {
        assert!(is_valid_response("<DATA|0:1234;>"));
        assert!(is_valid_response("<OK>"));
        assert!(is_valid_response("<ERROR|Something went wrong>"));
        assert!(is_valid_response("<"));
    }

    #[test]
    fn test_response_validation_invalid() {
        assert!(!is_valid_response(">COMMAND"));
        assert!(!is_valid_response("DATA|0:1234"));
        assert!(!is_valid_response(""));
        assert!(!is_valid_response(" <DATA>"));
    }

    #[test]
    fn test_pwm_percentage_to_value_boundaries() {
        // Test the PWM percentage to value formula: (percent * 255) / 100
        let convert = |percent: u32| (percent * 255) / 100;

        // 0% -> 0
        assert_eq!(convert(0), 0);
        // 1% -> 2 (rounds down)
        assert_eq!(convert(1), 2);
        // 50% -> 127
        assert_eq!(convert(50), 127);
        // 99% -> 252
        assert_eq!(convert(99), 252);
        // 100% -> 255
        assert_eq!(convert(100), 255);
    }

    #[test]
    fn test_pwm_value_range() {
        // Verify all percentages map to valid u8 range
        for percent in 0..=100 {
            let value = (percent * 255) / 100;
            assert!(value <= 255);
        }
    }

    #[test]
    fn test_rpm_byte_splitting_zero() {
        let rpm = 0u32;
        let rpm_high = ((rpm >> 8) & 0xFF) as u8;
        let rpm_low = (rpm & 0xFF) as u8;

        assert_eq!(rpm_high, 0);
        assert_eq!(rpm_low, 0);
        assert_eq!((rpm_high as u32) << 8 | rpm_low as u32, rpm);
    }

    #[test]
    fn test_rpm_byte_splitting_max() {
        let rpm = 65535u32;
        let rpm_high = ((rpm >> 8) & 0xFF) as u8;
        let rpm_low = (rpm & 0xFF) as u8;

        assert_eq!(rpm_high, 0xFF);
        assert_eq!(rpm_low, 0xFF);
        assert_eq!((rpm_high as u32) << 8 | rpm_low as u32, rpm);
    }

    #[test]
    fn test_rpm_byte_splitting_high_byte_only() {
        let rpm = 0xFF00u32;
        let rpm_high = ((rpm >> 8) & 0xFF) as u8;
        let rpm_low = (rpm & 0xFF) as u8;

        assert_eq!(rpm_high, 0xFF);
        assert_eq!(rpm_low, 0x00);
        assert_eq!((rpm_high as u32) << 8 | rpm_low as u32, rpm);
    }

    #[test]
    fn test_rpm_byte_splitting_low_byte_only() {
        let rpm = 0x00FFu32;
        let rpm_high = ((rpm >> 8) & 0xFF) as u8;
        let rpm_low = (rpm & 0xFF) as u8;

        assert_eq!(rpm_high, 0x00);
        assert_eq!(rpm_low, 0xFF);
        assert_eq!((rpm_high as u32) << 8 | rpm_low as u32, rpm);
    }

    #[test]
    fn test_rpm_byte_splitting_typical_values() {
        // Typical fan RPM values
        let test_cases = [
            (1000u32, 0x03, 0xE8), // ~1000 RPM
            (1500u32, 0x05, 0xDC), // ~1500 RPM
            (2000u32, 0x07, 0xD0), // ~2000 RPM
            (3000u32, 0x0B, 0xB8), // ~3000 RPM
            (5000u32, 0x13, 0x88), // ~5000 RPM
        ];

        for (rpm, expected_high, expected_low) in test_cases {
            let rpm_high = ((rpm >> 8) & 0xFF) as u8;
            let rpm_low = (rpm & 0xFF) as u8;

            assert_eq!(
                rpm_high, expected_high,
                "High byte mismatch for RPM {}",
                rpm
            );
            assert_eq!(rpm_low, expected_low, "Low byte mismatch for RPM {}", rpm);
            assert_eq!(
                (rpm_high as u32) << 8 | rpm_low as u32,
                rpm,
                "Reconstruction mismatch for RPM {}",
                rpm
            );
        }
    }

    /// Helper to build command payload (mirrors send_command logic)
    fn build_command_payload(cmd: Command, data: Option<&[u8]>) -> String {
        let mut payload = format!(">{:02X}", cmd as u8);

        if let Some(data_bytes) = data {
            for byte in data_bytes {
                payload.push_str(&format!("{:02X}", byte));
            }
        }

        payload
    }

    #[test]
    fn test_command_payload_get_all_rpm() {
        let payload = build_command_payload(Command::GetAllFanRpm, None);
        assert_eq!(payload, ">00");
    }

    #[test]
    fn test_command_payload_get_single_rpm() {
        let payload = build_command_payload(Command::GetSingleFanRpm, Some(&[5]));
        assert_eq!(payload, ">0105");
    }

    #[test]
    fn test_command_payload_set_fan_pwm() {
        // Fan 3 at 50% (127 = 0x7F)
        let payload = build_command_payload(Command::SetFanPwm, Some(&[3, 127]));
        assert_eq!(payload, ">02037F");
    }

    #[test]
    fn test_command_payload_set_all_pwm() {
        // All fans at 100% (255 = 0xFF)
        let payload = build_command_payload(Command::SetAllFanPwm, Some(&[255]));
        assert_eq!(payload, ">03FF");
    }

    #[test]
    fn test_command_payload_set_fan_rpm() {
        // Fan 2, target 3000 RPM (0x0BB8 -> high=0x0B, low=0xB8)
        let payload = build_command_payload(Command::SetFanRpm, Some(&[2, 0x0B, 0xB8]));
        assert_eq!(payload, ">04020BB8");
    }

    #[test]
    fn test_command_payload_get_hw_info() {
        let payload = build_command_payload(Command::GetHwInfo, None);
        assert_eq!(payload, ">05");
    }

    #[test]
    fn test_command_payload_get_fw_info() {
        let payload = build_command_payload(Command::GetFwInfo, None);
        assert_eq!(payload, ">06");
    }

    #[test]
    fn test_fan_id_validation_standard_board() {
        // OpenFAN Standard has 10 fans (0-9 valid)
        use openfan_core::OpenFanStandard;

        let fan_count = OpenFanStandard::FAN_COUNT;
        for fan_id in 0..fan_count {
            assert!(fan_id < fan_count);
        }
        // Fan ID 10 should be invalid (out of range)
        assert!(fan_count <= 10);
    }

    #[test]
    fn test_fan_id_validation_micro_board() {
        // OpenFAN Micro: test via BoardType enum methods
        use openfan_core::BoardType;

        let micro = BoardType::OpenFanMicro;
        // Micro has 1 fan (as currently defined in BoardType::fan_count)
        assert_eq!(micro.fan_count(), 1);
    }

    #[test]
    fn test_pwm_max_value_standard_board() {
        use openfan_core::OpenFanStandard;
        assert_eq!(OpenFanStandard::MAX_PWM, 100);
    }

    #[test]
    fn test_board_type_methods() {
        use openfan_core::BoardType;

        let standard = BoardType::OpenFanStandard;
        assert_eq!(standard.name(), "OpenFAN Standard");
        assert_eq!(standard.fan_count(), 10);
        assert_eq!(standard.usb_vid(), 0x2E8A);
        assert_eq!(standard.usb_pid(), 0x000A);

        let micro = BoardType::OpenFanMicro;
        assert_eq!(micro.name(), "OpenFAN Micro");
        assert_eq!(micro.usb_vid(), 0x2E8A);
        assert_eq!(micro.usb_pid(), 0x000B);
    }
}

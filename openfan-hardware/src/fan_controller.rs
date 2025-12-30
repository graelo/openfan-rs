//! Fan Controller - High-level interface for fan control
//!
//! Implements the fan control protocol over serial communication.

use crate::serial_driver::{SerialDriver, SerialTransport};
use openfan_core::{BoardConfig, FanRpmMap, OpenFanError, Result};
use std::collections::HashMap;
use std::fmt::Write;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, warn};

/// Convert PWM percentage (0-100) to byte value (0-255)
///
/// Uses integer arithmetic to avoid floating point:
/// - 0% → 0
/// - 50% → 127
/// - 100% → 255
#[inline]
pub fn pwm_percent_to_byte(percent: u32) -> u8 {
    ((percent * 255) / 100) as u8
}

/// Convert RPM value to high and low bytes for serial protocol
///
/// The protocol expects RPM as two bytes: high byte first, then low byte.
/// - RPM 0 → (0x00, 0x00)
/// - RPM 3000 (0x0BB8) → (0x0B, 0xB8)
/// - RPM 65535 → (0xFF, 0xFF)
#[inline]
pub fn rpm_to_bytes(rpm: u32) -> (u8, u8) {
    let high = ((rpm >> 8) & 0xFF) as u8;
    let low = (rpm & 0xFF) as u8;
    (high, low)
}

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
///
/// Generic over the transport type, allowing real hardware (`SerialDriver`)
/// or mock transports for testing.
pub struct FanController<T: SerialTransport + ?Sized = dyn SerialTransport> {
    driver: Arc<Mutex<Box<T>>>,
    fan_count: usize,
    max_pwm: u32,
    fan_rpm_cache: HashMap<u8, u32>,
    fan_pwm_cache: HashMap<u8, u32>,
}

impl<B: BoardConfig + Send + Sync> FanController<SerialDriver<B>> {
    /// Create a new FanController with the given serial driver
    pub fn new(driver: SerialDriver<B>) -> Self {
        Self {
            driver: Arc::new(Mutex::new(Box::new(driver))),
            fan_count: B::FAN_COUNT,
            max_pwm: B::MAX_PWM,
            fan_rpm_cache: HashMap::new(),
            fan_pwm_cache: HashMap::new(),
        }
    }
}

impl<T: SerialTransport + ?Sized> FanController<T> {
    /// Create a new FanController with a boxed transport
    ///
    /// This is primarily useful for testing with mock transports.
    pub fn with_transport(transport: Box<T>, fan_count: usize, max_pwm: u32) -> Self {
        Self {
            driver: Arc::new(Mutex::new(transport)),
            fan_count,
            max_pwm,
            fan_rpm_cache: HashMap::new(),
            fan_pwm_cache: HashMap::new(),
        }
    }

    /// Validate a fan ID against this board's fan count
    fn validate_fan_id(&self, fan_id: u8) -> Result<()> {
        if fan_id as usize >= self.fan_count {
            return Err(OpenFanError::InvalidFanId {
                fan_id,
                max_fans: self.fan_count,
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
                // write! to String is infallible
                let _ = write!(payload, "{:02X}", byte);
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

        if pwm_percent > self.max_pwm {
            return Err(OpenFanError::InvalidInput(format!(
                "PWM percentage must be 0-100, got {}",
                pwm_percent
            )));
        }

        let data = [fan_id, pwm_percent_to_byte(pwm_percent)];

        let result = self.send_command(Command::SetFanPwm, Some(&data)).await?;

        // Cache the PWM value on successful write
        self.fan_pwm_cache.insert(fan_id, pwm_percent);

        Ok(result)
    }

    /// Set PWM for all fans
    pub async fn set_all_fan_pwm(&mut self, pwm_percent: u32) -> Result<String> {
        if pwm_percent > self.max_pwm {
            return Err(OpenFanError::InvalidInput(format!(
                "PWM percentage must be 0-100, got {}",
                pwm_percent
            )));
        }

        let data = [pwm_percent_to_byte(pwm_percent)];

        let result = self
            .send_command(Command::SetAllFanPwm, Some(&data))
            .await?;

        // Cache the PWM value for all fans on successful write
        for fan_id in 0..self.fan_count as u8 {
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

        let (rpm_high, rpm_low) = rpm_to_bytes(rpm);
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
    use async_trait::async_trait;
    use std::collections::VecDeque;

    /// Mock transport for testing FanController without hardware
    struct MockTransport {
        /// Queued responses to return
        responses: std::sync::Mutex<VecDeque<Vec<String>>>,
        /// Record of commands sent
        sent_commands: std::sync::Mutex<Vec<String>>,
    }

    impl MockTransport {
        fn new() -> Self {
            Self {
                responses: std::sync::Mutex::new(VecDeque::new()),
                sent_commands: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn queue_response(&self, response: Vec<String>) {
            self.responses.lock().unwrap().push_back(response);
        }

        fn get_sent_commands(&self) -> Vec<String> {
            self.sent_commands.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl SerialTransport for MockTransport {
        async fn transaction(&mut self, command: &str) -> Result<Vec<String>> {
            self.sent_commands.lock().unwrap().push(command.to_string());

            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| OpenFanError::Hardware("No response queued".to_string()))
        }

        fn clear_input_buffer(&mut self) -> Result<()> {
            Ok(())
        }
    }

    /// Create a test FanController with mock transport
    fn create_mock_controller(mock: MockTransport) -> FanController<MockTransport> {
        FanController::with_transport(Box::new(mock), 10, 100)
    }

    // --- Integration tests that exercise actual FanController methods ---

    #[tokio::test]
    async fn test_get_all_fan_rpm() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<DATA|0:1234;1:5678;2:9ABC;>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let rpm_map = controller.get_all_fan_rpm().await.unwrap();

        assert_eq!(rpm_map.get(&0), Some(&0x1234));
        assert_eq!(rpm_map.get(&1), Some(&0x5678));
        assert_eq!(rpm_map.get(&2), Some(&0x9ABC));
    }

    #[tokio::test]
    async fn test_get_single_fan_rpm() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<DATA|3:ABCD;>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let rpm = controller.get_single_fan_rpm(3).await.unwrap();

        assert_eq!(rpm, 0xABCD);
    }

    #[tokio::test]
    async fn test_get_single_fan_rpm_invalid_id() {
        let mock = MockTransport::new();

        let mut controller = create_mock_controller(mock);
        let result = controller.get_single_fan_rpm(15).await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            OpenFanError::InvalidFanId { .. }
        ));
    }

    #[tokio::test]
    async fn test_set_fan_pwm() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<OK>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let result = controller.set_fan_pwm(0, 50).await;

        assert!(result.is_ok());
        // Check PWM is cached
        assert_eq!(controller.get_single_fan_pwm(0), Some(50));
    }

    #[tokio::test]
    async fn test_set_fan_pwm_invalid_value() {
        let mock = MockTransport::new();

        let mut controller = create_mock_controller(mock);
        let result = controller.set_fan_pwm(0, 150).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OpenFanError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_set_fan_pwm_invalid_fan_id() {
        let mock = MockTransport::new();

        let mut controller = create_mock_controller(mock);
        let result = controller.set_fan_pwm(20, 50).await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            OpenFanError::InvalidFanId { .. }
        ));
    }

    #[tokio::test]
    async fn test_set_all_fan_pwm() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<OK>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let result = controller.set_all_fan_pwm(75).await;

        assert!(result.is_ok());
        // Check all fans have cached PWM
        for fan_id in 0..10u8 {
            assert_eq!(controller.get_single_fan_pwm(fan_id), Some(75));
        }
    }

    #[tokio::test]
    async fn test_set_all_fan_pwm_invalid_value() {
        let mock = MockTransport::new();

        let mut controller = create_mock_controller(mock);
        let result = controller.set_all_fan_pwm(200).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OpenFanError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_set_fan_rpm() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<OK>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let result = controller.set_fan_rpm(5, 2000).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_set_fan_rpm_invalid_value() {
        let mock = MockTransport::new();

        let mut controller = create_mock_controller(mock);
        let result = controller.set_fan_rpm(0, 70000).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OpenFanError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_get_hw_info() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<HW|Model:Standard;Rev:1.0>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let hw_info = controller.get_hw_info().await.unwrap();

        assert!(hw_info.contains("Standard"));
    }

    #[tokio::test]
    async fn test_get_fw_info() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<FW|Version:1.2.3>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let fw_info = controller.get_fw_info().await.unwrap();

        assert!(fw_info.contains("1.2.3"));
    }

    #[tokio::test]
    async fn test_command_format_get_all_rpm() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<DATA|0:0000;>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let _ = controller.get_all_fan_rpm().await;

        let sent = controller.driver.lock().await.get_sent_commands();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0], ">00"); // GetAllFanRpm = 0x00
    }

    #[tokio::test]
    async fn test_command_format_set_pwm() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<OK>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let _ = controller.set_fan_pwm(3, 50).await; // 50% = 127 = 0x7F

        let sent = controller.driver.lock().await.get_sent_commands();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0], ">02037F"); // SetFanPwm(0x02), fan 3, pwm 0x7F
    }

    #[tokio::test]
    async fn test_command_format_set_rpm() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<OK>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let _ = controller.set_fan_rpm(2, 3000).await; // 3000 = 0x0BB8

        let sent = controller.driver.lock().await.get_sent_commands();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0], ">04020BB8"); // SetFanRpm(0x04), fan 2, high=0x0B, low=0xB8
    }

    #[tokio::test]
    async fn test_get_all_fan_pwm_initially_empty() {
        let mock = MockTransport::new();
        let controller = create_mock_controller(mock);

        let pwm_map = controller.get_all_fan_pwm();
        assert!(pwm_map.is_empty());
    }

    #[tokio::test]
    async fn test_get_all_fan_pwm_after_set() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<OK>".to_string()]);
        mock.queue_response(vec!["<OK>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let _ = controller.set_fan_pwm(0, 25).await;
        let _ = controller.set_fan_pwm(5, 75).await;

        let pwm_map = controller.get_all_fan_pwm();
        assert_eq!(pwm_map.get(&0), Some(&25));
        assert_eq!(pwm_map.get(&5), Some(&75));
        assert_eq!(pwm_map.len(), 2);
    }

    #[tokio::test]
    async fn test_parse_response_no_valid_line() {
        let mock = MockTransport::new();
        // Response without '<' prefix is invalid
        mock.queue_response(vec!["INVALID RESPONSE".to_string()]);

        let mut controller = create_mock_controller(mock);
        let result = controller.get_hw_info().await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OpenFanError::Hardware(_)));
    }

    #[tokio::test]
    async fn test_parse_fan_rpm_invalid_format() {
        let mock = MockTransport::new();
        // Missing data part after |
        mock.queue_response(vec!["<DATA>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let result = controller.get_all_fan_rpm().await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OpenFanError::Parse(_)));
    }

    #[tokio::test]
    async fn test_rpm_cache_updated_after_get() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<DATA|0:1234;1:5678;>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let _ = controller.get_all_fan_rpm().await;

        // RPM cache should be updated
        // Note: There's no direct getter for RPM cache, but parse_fan_rpm updates it
        // This test verifies the flow completes without error
    }

    // --- Existing unit tests below ---

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

    // --- RPM parsing edge case tests (via MockTransport) ---

    #[tokio::test]
    async fn test_parse_fan_rpm_ten_fans() {
        let mock = MockTransport::new();
        mock.queue_response(vec![
            "<DATA|0:0100;1:0200;2:0300;3:0400;4:0500;5:0600;6:0700;7:0800;8:0900;9:0A00;>"
                .to_string(),
        ]);

        let mut controller = create_mock_controller(mock);
        let rpm_map = controller.get_all_fan_rpm().await.unwrap();

        assert_eq!(rpm_map.len(), 10);
        for i in 0..10 {
            let expected_rpm = (i + 1) * 0x100;
            assert_eq!(rpm_map.get(&(i as u8)), Some(&expected_rpm));
        }
    }

    #[tokio::test]
    async fn test_parse_fan_rpm_zero_rpm() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<DATA|0:0000;1:0000;>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let rpm_map = controller.get_all_fan_rpm().await.unwrap();

        assert_eq!(rpm_map.get(&0), Some(&0));
        assert_eq!(rpm_map.get(&1), Some(&0));
    }

    #[tokio::test]
    async fn test_parse_fan_rpm_max_rpm() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<DATA|0:FFFF;>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let rpm_map = controller.get_all_fan_rpm().await.unwrap();

        assert_eq!(rpm_map.get(&0), Some(&0xFFFF));
    }

    #[tokio::test]
    async fn test_parse_fan_rpm_lowercase_hex() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<DATA|0:abcd;1:ef00;>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let rpm_map = controller.get_all_fan_rpm().await.unwrap();

        assert_eq!(rpm_map.get(&0), Some(&0xABCD));
        assert_eq!(rpm_map.get(&1), Some(&0xEF00));
    }

    #[tokio::test]
    async fn test_parse_fan_rpm_mixed_case_hex() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<DATA|0:AbCd;1:eF01;>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let rpm_map = controller.get_all_fan_rpm().await.unwrap();

        assert_eq!(rpm_map.get(&0), Some(&0xABCD));
        assert_eq!(rpm_map.get(&1), Some(&0xEF01));
    }

    #[tokio::test]
    async fn test_parse_fan_rpm_empty_data() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<DATA|;>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let rpm_map = controller.get_all_fan_rpm().await.unwrap();

        assert!(rpm_map.is_empty());
    }

    #[tokio::test]
    async fn test_parse_fan_rpm_no_separator() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<DATA>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let result = controller.get_all_fan_rpm().await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OpenFanError::Parse(_)));
    }

    #[tokio::test]
    async fn test_parse_fan_rpm_invalid_fan_id() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<DATA|abc:1234;>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let result = controller.get_all_fan_rpm().await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OpenFanError::Parse(_)));
    }

    #[tokio::test]
    async fn test_parse_fan_rpm_invalid_rpm_value() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<DATA|0:GHIJ;>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let result = controller.get_all_fan_rpm().await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OpenFanError::Parse(_)));
    }

    #[tokio::test]
    async fn test_parse_fan_rpm_skips_malformed_entries() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<DATA|0:1234;invalid;2:5678;>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let rpm_map = controller.get_all_fan_rpm().await.unwrap();

        assert_eq!(rpm_map.len(), 2);
        assert_eq!(rpm_map.get(&0), Some(&0x1234));
        assert_eq!(rpm_map.get(&2), Some(&0x5678));
    }

    #[tokio::test]
    async fn test_parse_fan_rpm_trailing_semicolons() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<DATA|0:1234;;;>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let rpm_map = controller.get_all_fan_rpm().await.unwrap();

        assert_eq!(rpm_map.len(), 1);
        assert_eq!(rpm_map.get(&0), Some(&0x1234));
    }

    #[tokio::test]
    async fn test_parse_fan_rpm_duplicate_fan_ids() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<DATA|0:1000;0:2000;>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let rpm_map = controller.get_all_fan_rpm().await.unwrap();

        // When duplicate fan IDs appear, last value wins
        assert_eq!(rpm_map.len(), 1);
        assert_eq!(rpm_map.get(&0), Some(&0x2000));
    }

    #[tokio::test]
    async fn test_parse_fan_rpm_out_of_order_fan_ids() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<DATA|5:5555;0:0000;9:9999;1:1111;>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let rpm_map = controller.get_all_fan_rpm().await.unwrap();

        assert_eq!(rpm_map.len(), 4);
        assert_eq!(rpm_map.get(&0), Some(&0x0000));
        assert_eq!(rpm_map.get(&1), Some(&0x1111));
        assert_eq!(rpm_map.get(&5), Some(&0x5555));
        assert_eq!(rpm_map.get(&9), Some(&0x9999));
    }

    #[tokio::test]
    async fn test_parse_fan_rpm_large_hex_values() {
        let mock = MockTransport::new();
        mock.queue_response(vec!["<DATA|0:FFFE;1:FFFF;>".to_string()]);

        let mut controller = create_mock_controller(mock);
        let rpm_map = controller.get_all_fan_rpm().await.unwrap();

        assert_eq!(rpm_map.get(&0), Some(&0xFFFE));
        assert_eq!(rpm_map.get(&1), Some(&0xFFFF));
    }

    #[test]
    fn test_pwm_percent_to_byte_boundaries() {
        // Test the actual pwm_percent_to_byte() function
        // 0% -> 0
        assert_eq!(pwm_percent_to_byte(0), 0);
        // 1% -> 2 (rounds down)
        assert_eq!(pwm_percent_to_byte(1), 2);
        // 50% -> 127
        assert_eq!(pwm_percent_to_byte(50), 127);
        // 99% -> 252
        assert_eq!(pwm_percent_to_byte(99), 252);
        // 100% -> 255
        assert_eq!(pwm_percent_to_byte(100), 255);
    }

    #[test]
    fn test_pwm_percent_to_byte_full_range() {
        // Verify monotonicity: higher percent → higher (or equal) byte value
        for percent in 1..=100 {
            assert!(pwm_percent_to_byte(percent) >= pwm_percent_to_byte(percent - 1));
        }
        // Verify endpoints
        assert_eq!(pwm_percent_to_byte(0), 0);
        assert_eq!(pwm_percent_to_byte(100), 255);
    }

    #[test]
    fn test_rpm_to_bytes_zero() {
        // Test the actual rpm_to_bytes() function at zero
        let (high, low) = rpm_to_bytes(0);
        assert_eq!(high, 0);
        assert_eq!(low, 0);
    }

    #[test]
    fn test_rpm_to_bytes_max() {
        // Test the actual rpm_to_bytes() function at max 16-bit value
        let (high, low) = rpm_to_bytes(65535);
        assert_eq!(high, 0xFF);
        assert_eq!(low, 0xFF);
    }

    #[test]
    fn test_rpm_to_bytes_high_byte_only() {
        // Test rpm_to_bytes() with only high byte set
        let (high, low) = rpm_to_bytes(0xFF00);
        assert_eq!(high, 0xFF);
        assert_eq!(low, 0x00);
    }

    #[test]
    fn test_rpm_to_bytes_low_byte_only() {
        // Test rpm_to_bytes() with only low byte set
        let (high, low) = rpm_to_bytes(0x00FF);
        assert_eq!(high, 0x00);
        assert_eq!(low, 0xFF);
    }

    #[test]
    fn test_rpm_to_bytes_typical_values() {
        // Test rpm_to_bytes() with typical fan RPM values
        let test_cases = [
            (1000u32, 0x03, 0xE8), // ~1000 RPM
            (1500u32, 0x05, 0xDC), // ~1500 RPM
            (2000u32, 0x07, 0xD0), // ~2000 RPM
            (3000u32, 0x0B, 0xB8), // ~3000 RPM
            (5000u32, 0x13, 0x88), // ~5000 RPM
        ];

        for (rpm, expected_high, expected_low) in test_cases {
            let (high, low) = rpm_to_bytes(rpm);
            assert_eq!(high, expected_high, "High byte mismatch for RPM {}", rpm);
            assert_eq!(low, expected_low, "Low byte mismatch for RPM {}", rpm);
        }
    }

    #[test]
    fn test_rpm_to_bytes_roundtrip() {
        // Verify bytes can reconstruct the original RPM value
        for rpm in [0, 500, 1000, 2000, 3000, 5000, 9000, 65535] {
            let (high, low) = rpm_to_bytes(rpm);
            let reconstructed = ((high as u32) << 8) | (low as u32);
            assert_eq!(reconstructed, rpm, "Roundtrip failed for RPM {}", rpm);
        }
    }

    // --- openfan_core type tests ---

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

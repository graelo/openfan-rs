//! Fan Commander - High-level interface for fan control
//!
//! Implements the fan control protocol over serial communication.

use super::serial_driver::SerialDriver;
use openfan_core::{FanRpmMap, OpenFanError, Result};
use std::collections::HashMap;
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
pub struct FanCommander {
    driver: Arc<Mutex<SerialDriver>>,
    fan_rpm_cache: HashMap<u8, u32>,
}

impl FanCommander {
    /// Create a new FanCommander with the given serial driver
    pub fn new(driver: SerialDriver) -> Self {
        Self {
            driver: Arc::new(Mutex::new(driver)),
            fan_rpm_cache: HashMap::new(),
        }
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

    /// Get RPM for a single fan
    pub async fn get_single_fan_rpm(&mut self, fan_id: u8) -> Result<u32> {
        if fan_id >= 10 {
            return Err(OpenFanError::InvalidFanId(fan_id));
        }

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

    /// Set PWM for a single fan
    pub async fn set_fan_pwm(&mut self, fan_id: u8, pwm_percent: u32) -> Result<String> {
        if fan_id >= 10 {
            return Err(OpenFanError::InvalidFanId(fan_id));
        }

        if pwm_percent > 100 {
            return Err(OpenFanError::InvalidInput(format!(
                "PWM percentage must be 0-100, got {}",
                pwm_percent
            )));
        }

        // Convert percentage to 0-255 range
        let pwm_value = (pwm_percent * 255) / 100;
        let data = [fan_id, pwm_value as u8];

        self.send_command(Command::SetFanPwm, Some(&data)).await
    }

    /// Set PWM for all fans
    pub async fn set_all_fan_pwm(&mut self, pwm_percent: u32) -> Result<String> {
        if pwm_percent > 100 {
            return Err(OpenFanError::InvalidInput(format!(
                "PWM percentage must be 0-100, got {}",
                pwm_percent
            )));
        }

        // Convert percentage to 0-255 range
        let pwm_value = (pwm_percent * 255) / 100;
        let data = [pwm_value as u8];

        self.send_command(Command::SetAllFanPwm, Some(&data)).await
    }

    /// Set target RPM for a single fan
    pub async fn set_fan_rpm(&mut self, fan_id: u8, rpm: u32) -> Result<String> {
        if fan_id >= 10 {
            return Err(OpenFanError::InvalidFanId(fan_id));
        }

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
    fn test_parse_fan_rpm_parsing() {
        // Test just the parsing logic without creating a FanCommander
        let response = "<DATA|0:1234;1:5678;2:9ABC;>";

        // Parse manually to test the logic
        let parts: Vec<&str> = response.split('|').collect();
        assert_eq!(parts.len(), 2);

        let data_part = parts[1].trim_end_matches(';').trim_end_matches('>');
        let mut rpm_map = HashMap::new();

        for fan_data in data_part.split(';') {
            if fan_data.is_empty() {
                continue;
            }

            let fan_parts: Vec<&str> = fan_data.split(':').collect();
            if fan_parts.len() == 2 {
                let fan_id = fan_parts[0].parse::<u8>().unwrap();
                let rpm = u32::from_str_radix(fan_parts[1], 16).unwrap();
                rpm_map.insert(fan_id, rpm);
            }
        }

        assert_eq!(rpm_map.get(&0), Some(&0x1234));
        assert_eq!(rpm_map.get(&1), Some(&0x5678));
        assert_eq!(rpm_map.get(&2), Some(&0x9ABC));
    }

    #[test]
    fn test_pwm_validation() {
        // Test PWM percentage to value conversion
        assert_eq!((50 * 255) / 100, 127); // 50% -> 127
        assert_eq!((100 * 255) / 100, 255); // 100% -> 255
    }

    #[test]
    fn test_rpm_validation() {
        // Test RPM splitting
        let rpm = 1234u32;
        let rpm_high = ((rpm >> 8) & 0xFF) as u8;
        let rpm_low = (rpm & 0xFF) as u8;

        assert_eq!(rpm_high, 4); // 0x04
        assert_eq!(rpm_low, 210); // 0xD2
        assert_eq!((rpm_high as u32) << 8 | rpm_low as u32, rpm);
    }
}

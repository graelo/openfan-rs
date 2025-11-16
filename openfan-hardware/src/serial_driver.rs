//! Serial driver for low-level hardware communication
//!
//! Provides async serial I/O with the fan controller hardware.

use openfan_core::{BoardConfig, OpenFanError, Result};
use std::marker::PhantomData;
use std::time::Duration;
use tokio::time::timeout;
use tokio_serial::{SerialPort, SerialPortBuilderExt, SerialStream};
use tracing::{debug, error, warn};

/// Serial driver for hardware communication
pub struct SerialDriver<B: BoardConfig = openfan_core::DefaultBoard> {
    port: SerialStream,
    prefix: String,
    suffix: String,
    timeout_duration: Duration,
    debug_uart: bool,
    _board: PhantomData<B>,
}

impl<B: BoardConfig> SerialDriver<B> {
    /// Create a new serial driver
    ///
    /// # Arguments
    /// * `port_path` - Path to the serial device (e.g., "/dev/ttyACM0")
    /// * `timeout_ms` - Timeout in milliseconds for read/write operations
    /// * `debug_uart` - Enable UART debug logging
    pub fn new(port_path: &str, timeout_ms: u64, debug_uart: bool) -> Result<Self> {
        debug!("Opening serial port: {}", port_path);

        let port = tokio_serial::new(port_path, B::BAUD_RATE)
            .timeout(Duration::from_millis(timeout_ms))
            .data_bits(tokio_serial::DataBits::Eight)
            .parity(tokio_serial::Parity::None)
            .stop_bits(tokio_serial::StopBits::One)
            .flow_control(tokio_serial::FlowControl::None)
            .open_native_async()
            .map_err(|e| {
                error!("Failed to open serial port {}: {}", port_path, e);
                OpenFanError::Serial(format!("Failed to open serial port: {}", e))
            })?;

        debug!("Serial port opened successfully");

        if debug_uart {
            debug!("UART debug logging enabled");
        }

        Ok(Self {
            port,
            prefix: String::new(),
            suffix: "\r\n".to_string(),
            timeout_duration: Duration::from_millis(timeout_ms),
            debug_uart,
            _board: PhantomData,
        })
    }

    /// Send a command and wait for response
    ///
    /// This is the main transaction function that handles:
    /// 1. Flushing input buffer
    /// 2. Sending command with prefix/suffix
    /// 3. Reading response lines until one starts with '<'
    pub async fn transaction(&mut self, command: &str) -> Result<Vec<String>> {
        // Clear any pending input
        self.clear_input_buffer()?;

        // Send command
        self.send(command).await?;

        // Read response
        self.read_until_response().await
    }

    /// Send a command to the serial port
    async fn send(&mut self, command: &str) -> Result<()> {
        let full_command = format!("{}{}{}", self.prefix, command, self.suffix);

        if self.debug_uart {
            debug!("TX: {:?}", full_command);
        }

        use tokio::io::AsyncWriteExt;

        timeout(
            self.timeout_duration,
            self.port.write_all(full_command.as_bytes()),
        )
        .await
        .map_err(|_| {
            error!("Write timeout");
            OpenFanError::Timeout("Write operation timed out".to_string())
        })?
        .map_err(|e| {
            error!("Write failed: {}", e);
            OpenFanError::Serial(format!("Write failed: {}", e))
        })?;

        // Flush to ensure data is sent
        timeout(self.timeout_duration, self.port.flush())
            .await
            .map_err(|_| OpenFanError::Timeout("Flush operation timed out".to_string()))?
            .map_err(|e| OpenFanError::Serial(format!("Flush failed: {}", e)))?;

        Ok(())
    }

    /// Read lines until we get a response starting with '<'
    async fn read_until_response(&mut self) -> Result<Vec<String>> {
        use tokio::io::AsyncBufReadExt;
        use tokio::io::BufReader;

        let mut lines = Vec::new();
        let mut reader = BufReader::new(&mut self.port);

        // Read with timeout
        let result = timeout(self.timeout_duration, async {
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        // EOF
                        break;
                    }
                    Ok(_) => {
                        let line = line.trim().to_string();
                        if !line.is_empty() {
                            if self.debug_uart {
                                debug!("RX: {:?}", line);
                            }
                            lines.push(line.clone());

                            // Stop when we get a response line (starts with '<')
                            if line.starts_with('<') {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Read error: {}", e);
                        return Err(OpenFanError::Serial(format!("Read error: {}", e)));
                    }
                }
            }
            Ok(lines)
        })
        .await;

        match result {
            Ok(Ok(lines)) => Ok(lines),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                error!("Read timeout");
                Err(OpenFanError::Timeout(
                    "Read operation timed out".to_string(),
                ))
            }
        }
    }

    /// Clear the input buffer
    fn clear_input_buffer(&mut self) -> Result<()> {
        self.port
            .clear(tokio_serial::ClearBuffer::Input)
            .map_err(|e| {
                warn!("Failed to clear input buffer: {}", e);
                OpenFanError::Serial(format!("Failed to clear buffer: {}", e))
            })
    }
}

/// Find the fan controller device by VID/PID
///
/// Searches for device matching the board's USB VID/PID
pub fn find_fan_controller<B: BoardConfig>() -> Result<String> {
    debug!(
        "Searching for {} (VID:0x{:04X}, PID:0x{:04X})",
        B::NAME,
        B::USB_VID,
        B::USB_PID
    );

    let ports = tokio_serial::available_ports().map_err(|e| {
        error!("Failed to enumerate serial ports: {}", e);
        OpenFanError::Hardware(format!("Failed to enumerate ports: {}", e))
    })?;

    for port in ports {
        debug!("Checking port: {}", port.port_name);

        if let tokio_serial::SerialPortType::UsbPort(info) = &port.port_type {
            debug!("  USB Device - VID:{:04X} PID:{:04X}", info.vid, info.pid);

            if info.vid == B::USB_VID && info.pid == B::USB_PID {
                debug!("Found {} at: {}", B::NAME, port.port_name);
                return Ok(port.port_name);
            }
        }
    }

    error!("{} not found", B::NAME);
    Err(OpenFanError::DeviceNotFound)
}

/// Detect board type from USB VID/PID
///
/// Scans available serial ports and matches against known board USB identifiers.
///
/// # Errors
///
/// Returns an error if:
/// - No serial ports are found
/// - No matching OpenFAN device is detected
/// - Serial port enumeration fails
pub fn detect_board_from_usb() -> Result<openfan_core::BoardType> {
    use openfan_core::BoardType;

    let ports = tokio_serial::available_ports().map_err(|e| {
        error!("Failed to enumerate serial ports: {}", e);
        OpenFanError::Serial(format!("Failed to enumerate USB ports: {}", e))
    })?;

    for port in ports {
        if let tokio_serial::SerialPortType::UsbPort(info) = &port.port_type {
            match (info.vid, info.pid) {
                (0x2E8A, 0x000A) => {
                    debug!("Detected OpenFAN v1 at: {}", port.port_name);
                    return Ok(BoardType::OpenFanV1);
                }
                (0x2E8A, 0x000B) => {
                    debug!("Detected OpenFAN Mini at: {}", port.port_name);
                    return Ok(BoardType::OpenFanMini);
                }
                _ => continue,
            }
        }
    }

    error!("No OpenFAN device detected");
    Err(OpenFanError::DeviceNotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_ports() {
        // This test will only work if ports are available
        // Just check that the function doesn't panic
        let _ = tokio_serial::available_ports();
    }

    #[test]
    fn test_find_fan_controller() {
        // This will fail if hardware isn't connected, which is expected
        let result = find_fan_controller::<openfan_core::DefaultBoard>();
        // Just verify the function runs without panicking
        let _ = result;
    }
}

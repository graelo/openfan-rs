//! Serial driver for low-level hardware communication
//!
//! Provides async serial I/O with the fan controller hardware.

use async_trait::async_trait;
use openfan_core::{BoardConfig, OpenFanError, Result};
use std::marker::PhantomData;
use std::time::Duration;
use tokio::time::timeout;
use tokio_serial::{SerialPort, SerialPortBuilderExt, SerialStream};
use tracing::{debug, error, warn};

/// Trait for serial transport abstraction
///
/// This trait enables testing of `FanController` without real hardware
/// by allowing mock implementations.
#[async_trait]
pub trait SerialTransport: Send {
    /// Send a command and wait for response lines
    async fn transaction(&mut self, command: &str) -> Result<Vec<String>>;

    /// Clear the input buffer
    fn clear_input_buffer(&mut self) -> Result<()>;

    /// Check if the transport is connected
    fn is_connected(&self) -> bool;

    /// Get the port path for reconnection purposes
    fn port_path(&self) -> Option<&str>;
}

/// Serial driver for hardware communication
pub struct SerialDriver<B: BoardConfig = openfan_core::DefaultBoard> {
    port: SerialStream,
    port_path: String,
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
            port_path: port_path.to_string(),
            prefix: String::new(),
            suffix: "\r\n".to_string(),
            timeout_duration: Duration::from_millis(timeout_ms),
            debug_uart,
            _board: PhantomData,
        })
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
                        // EOF indicates device disconnection (USB unplugged, power loss, etc.)
                        warn!("Serial port returned EOF - device may have been disconnected");
                        return Err(OpenFanError::DeviceDisconnected(
                            "Serial port returned EOF - device may have been unplugged".to_string(),
                        ));
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
    fn clear_input_buffer_impl(&mut self) -> Result<()> {
        self.port
            .clear(tokio_serial::ClearBuffer::Input)
            .map_err(|e| {
                warn!("Failed to clear input buffer: {}", e);
                OpenFanError::Serial(format!("Failed to clear buffer: {}", e))
            })
    }
}

#[async_trait]
impl<B: BoardConfig + Send + Sync> SerialTransport for SerialDriver<B> {
    async fn transaction(&mut self, command: &str) -> Result<Vec<String>> {
        // Clear any pending input
        self.clear_input_buffer_impl()?;

        // Send command
        self.send(command).await?;

        // Read response
        self.read_until_response().await
    }

    fn clear_input_buffer(&mut self) -> Result<()> {
        self.clear_input_buffer_impl()
    }

    fn is_connected(&self) -> bool {
        // Check if the serial port is still valid by attempting to get port info
        // Note: This is a best-effort check; actual disconnection is detected during I/O
        true // SerialStream doesn't provide a direct "is open" check
    }

    fn port_path(&self) -> Option<&str> {
        Some(&self.port_path)
    }
}

/// Determine if an error indicates device disconnection
///
/// Returns `true` if the error suggests the device has been disconnected
/// (USB unplugged, power loss, etc.) rather than a transient error.
pub fn is_disconnect_error(err: &OpenFanError) -> bool {
    match err {
        OpenFanError::DeviceDisconnected(_) => true,
        OpenFanError::Serial(msg) | OpenFanError::Hardware(msg) => {
            let msg_lower = msg.to_lowercase();
            msg_lower.contains("broken pipe")
                || msg_lower.contains("no such device")
                || msg_lower.contains("resource temporarily unavailable")
                || msg_lower.contains("permission denied")
                || msg_lower.contains("device disconnected")
                || msg_lower.contains("device not configured")
                || msg_lower.contains("input/output error")
        }
        // Timeouts are typically transient, not disconnection
        OpenFanError::Timeout(_) => false,
        _ => false,
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
/// Currently only detects OpenFAN Standard boards via USB serial.
///
/// Note: Custom boards cannot be auto-detected and must be specified manually
/// with the `--board custom:N` flag.
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
            // OpenFAN Standard: VID=0x2E8A (Raspberry Pi), PID=0x000A
            if info.vid == 0x2E8A && info.pid == 0x000A {
                debug!("Detected OpenFAN Standard at: {}", port.port_name);
                return Ok(BoardType::OpenFanStandard);
            }
        }
    }

    error!("No OpenFAN device detected");
    Err(OpenFanError::DeviceNotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Hardware-dependent tests (find_ports, find_fan_controller, detect_board_from_usb)
    // are tested via integration tests with actual hardware or mocks.
    // Board type tests are in openfan-core/src/board.rs.

    #[test]
    fn test_is_disconnect_error_device_disconnected() {
        let err = OpenFanError::DeviceDisconnected("test".to_string());
        assert!(is_disconnect_error(&err));
    }

    #[test]
    fn test_is_disconnect_error_serial_broken_pipe() {
        let err = OpenFanError::Serial("Broken pipe".to_string());
        assert!(is_disconnect_error(&err));
    }

    #[test]
    fn test_is_disconnect_error_serial_no_such_device() {
        let err = OpenFanError::Serial("No such device".to_string());
        assert!(is_disconnect_error(&err));
    }

    #[test]
    fn test_is_disconnect_error_hardware_io_error() {
        let err = OpenFanError::Hardware("Input/output error".to_string());
        assert!(is_disconnect_error(&err));
    }

    #[test]
    fn test_is_disconnect_error_timeout_not_disconnect() {
        let err = OpenFanError::Timeout("Read timeout".to_string());
        assert!(!is_disconnect_error(&err));
    }

    #[test]
    fn test_is_disconnect_error_other_not_disconnect() {
        let err = OpenFanError::InvalidInput("Bad value".to_string());
        assert!(!is_disconnect_error(&err));
    }

    #[test]
    fn test_is_disconnect_error_serial_normal_error() {
        let err = OpenFanError::Serial("Write failed: some other error".to_string());
        assert!(!is_disconnect_error(&err));
    }

    #[test]
    fn test_detect_board_from_usb_returns_valid_result() {
        // This test exercises the detect_board_from_usb function.
        // In CI (no hardware): returns DeviceNotFound
        // With hardware: returns Ok(BoardType::OpenFanStandard)
        let result = detect_board_from_usb();

        match result {
            Ok(board_type) => {
                // Hardware is present - verify it's a valid board type
                assert_eq!(board_type.name(), "OpenFAN Standard");
            }
            Err(OpenFanError::DeviceNotFound) => {
                // No hardware present - this is expected in CI
            }
            Err(other) => {
                panic!("Unexpected error from detect_board_from_usb: {:?}", other);
            }
        }
    }
}

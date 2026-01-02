//! Hardware abstraction layer for fan controller
//!
//! Re-export the hardware interface from the `openfan_hardware` crate so
//! consumers of `openfand` can access the hardware APIs without
//! depending on the internal module layout.

mod connection_manager;

pub use connection_manager::{ConnectionManager, ConnectionState};
pub use openfan_hardware::{
    detect_board_from_usb, find_fan_controller, FanController, SerialDriver,
};

/// Type alias for the standard fan controller with default board
pub type DefaultFanController = FanController<SerialDriver<openfan_core::DefaultBoard>>;

/// Hardware initialization and connection utilities
pub(crate) mod connection {
    use super::*;
    use openfan_core::{DefaultBoard, OpenFanError, Result};
    use std::env;
    use tracing::{debug, info, warn};

    /// Connect to a specific serial device
    ///
    /// Use this when the device path is known (e.g., from --device flag).
    /// Bypasses USB VID/PID detection.
    pub async fn connect_to_device(
        device_path: &str,
        timeout_ms: u64,
        debug_uart: bool,
    ) -> Result<DefaultFanController> {
        info!("Connecting to device: {}", device_path);

        let driver = SerialDriver::<DefaultBoard>::new(device_path, timeout_ms, debug_uart)
            .map_err(|e| {
                OpenFanError::Serial(format!("Failed to connect to {}: {}", device_path, e))
            })?;

        info!("Successfully connected to {}", device_path);
        Ok(FanController::new(driver))
    }

    /// Initialize hardware connection with automatic device detection
    ///
    /// Tries multiple methods to find and connect to the fan controller:
    /// 1. Search by board VID/PID (auto-detected)
    /// 2. Use OPENFAN_COMPORT environment variable
    /// 3. Try common device paths
    pub async fn auto_connect(timeout_ms: u64, debug_uart: bool) -> Result<DefaultFanController> {
        info!("Initializing hardware connection...");

        // Method 1: Auto-detect by VID/PID
        match find_fan_controller::<DefaultBoard>() {
            Ok(port_path) => {
                info!("Found fan controller at: {}", port_path);
                match SerialDriver::new(&port_path, timeout_ms, debug_uart) {
                    Ok(driver) => {
                        info!("Successfully connected to fan controller");
                        return Ok(FanController::new(driver));
                    }
                    Err(e) => {
                        warn!("Failed to connect to detected device: {}", e);
                    }
                }
            }
            Err(e) => {
                debug!("Auto-detection failed: {}", e);
            }
        }

        // Method 2: Try environment variable
        if let Ok(port_path) = env::var("OPENFAN_COMPORT") {
            info!("Trying port from OPENFAN_COMPORT: {}", port_path);
            match SerialDriver::new(&port_path, timeout_ms, debug_uart) {
                Ok(driver) => {
                    info!("Successfully connected via OPENFAN_COMPORT");
                    return Ok(FanController::new(driver));
                }
                Err(e) => {
                    warn!("Failed to connect to OPENFAN_COMPORT device: {}", e);
                }
            }
        }

        // Method 3: Try common device paths
        let common_paths = [
            "/dev/ttyACM0",
            "/dev/ttyACM1",
            "/dev/ttyUSB0",
            "/dev/ttyUSB1",
            "COM3",
            "COM4",
            "COM5",
        ];

        for path in &common_paths {
            debug!("Trying common path: {}", path);
            match SerialDriver::new(path, timeout_ms, debug_uart) {
                Ok(driver) => {
                    info!("Successfully connected to {}", path);
                    return Ok(FanController::new(driver));
                }
                Err(e) => {
                    debug!("Failed to connect to {}: {}", path, e);
                    continue;
                }
            }
        }

        Err(OpenFanError::DeviceNotFound)
    }

    /// Test hardware connection by getting firmware info
    pub async fn test_connection(controller: &mut DefaultFanController) -> Result<()> {
        info!("Testing hardware connection...");

        match controller.get_fw_info().await {
            Ok(fw_info) => {
                info!("Hardware test successful. Firmware: {}", fw_info);
                Ok(())
            }
            Err(e) => {
                warn!("Hardware test failed: {}", e);
                Err(e)
            }
        }
    }
}

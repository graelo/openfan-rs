//! Hardware abstraction layer for fan controller
//!
//! Re-export the hardware interface from the `openfan_hardware` crate so
//! consumers of `openfand` can access the hardware APIs without
//! depending on the internal module layout.

mod connection_manager;
mod controller_registry;

pub use connection_manager::{ConnectionManager, ConnectionState};
pub use controller_registry::{ControllerEntry, ControllerRegistry};
pub use openfan_hardware::{FanController, SerialDriver};

/// Type alias for the standard fan controller with default board
pub type DefaultFanController = FanController<SerialDriver<openfan_core::DefaultBoard>>;

/// Hardware initialization and connection utilities
pub(crate) mod connection {
    use super::*;
    use openfan_core::{DefaultBoard, OpenFanError, Result};
    use tracing::{info, warn};

    /// Connect to a specific serial device
    ///
    /// Use this when the device path is known (e.g., from --device flag or config).
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

    #[cfg(test)]
    mod tests {
        use super::*;

        #[tokio::test]
        async fn test_connect_to_device_invalid_path() {
            // Test that connecting to a non-existent device returns an error
            let result = connect_to_device("/dev/nonexistent_device_12345", 1000, false).await;

            match result {
                Err(OpenFanError::Serial(msg)) => {
                    assert!(
                        msg.contains("/dev/nonexistent_device_12345"),
                        "Error message should contain device path: {}",
                        msg
                    );
                }
                Err(other) => panic!("Expected Serial error, got {:?}", other),
                Ok(_) => panic!("Expected error for non-existent device"),
            }
        }
    }
}

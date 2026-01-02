//! openfan-hardware
//!
//! Hardware abstraction crate that contains the low-level serial driver and the
//! high-level fan controller logic. This crate is intended to be used by the
//! server and CLI crates to interact with the OpenFAN hardware.
//
//! Public API:
//! - `fan_controller::FanController` — high-level controller for fan operations
//! - `serial_driver::SerialDriver` — low-level serial I/O driver
//! - `serial_driver::find_fan_controller` — helper to locate the device by VID/PID
//! - `serial_driver::detect_board_from_usb` — detect board type from USB VID/PID

// Re-export modules so consumers can use `openfan_hardware::FanController` and
// `openfan_hardware::SerialDriver`.
pub mod fan_controller;
pub mod serial_driver;

// Re-export with default board type for convenience
pub type DefaultFanController = fan_controller::FanController<openfan_core::DefaultBoard>;
pub type DefaultSerialDriver = serial_driver::SerialDriver<openfan_core::DefaultBoard>;

pub use fan_controller::FanController;
pub use serial_driver::{
    detect_board_from_usb, find_fan_controller, is_disconnect_error, SerialDriver, SerialTransport,
};

#[cfg(test)]
mod tests {
    // Basic smoke tests to ensure the crate compiles and the public items are exposed.
    use super::*;

    #[test]
    fn exports_present() {
        // Ensure types are accessible (no runtime behavior required here).
        let _ = std::any::TypeId::of::<FanController>();
        let _ = std::any::TypeId::of::<SerialDriver>();
    }
}

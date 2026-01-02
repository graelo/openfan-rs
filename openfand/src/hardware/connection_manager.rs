//! Connection manager with automatic reconnection support
//!
//! This module provides a wrapper around `FanController` that handles
//! device disconnections and automatic reconnection with exponential backoff.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use openfan_core::{OpenFanError, ReconnectConfig, Result};
use openfan_hardware::is_disconnect_error;
use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use super::connection;
use super::DefaultFanController;

/// Connection state machine states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Device is connected and operational
    Connected,
    /// Device has been disconnected
    Disconnected,
    /// Reconnection is in progress
    Reconnecting { attempt: u32 },
}

impl ConnectionState {
    /// Get a string representation for API responses
    pub fn as_str(&self) -> &'static str {
        match self {
            ConnectionState::Connected => "connected",
            ConnectionState::Disconnected => "disconnected",
            ConnectionState::Reconnecting { .. } => "reconnecting",
        }
    }
}

/// Manages device connection with automatic reconnection support
pub struct ConnectionManager {
    /// The fan controller (None when disconnected)
    controller: RwLock<Option<DefaultFanController>>,
    /// Current connection state
    state: RwLock<ConnectionState>,
    /// Reconnection configuration
    config: ReconnectConfig,
    /// Serial communication timeout in milliseconds
    timeout_ms: u64,
    /// Enable UART debug logging
    debug_uart: bool,
    /// Cached PWM values to restore after reconnection
    pwm_cache: Mutex<HashMap<u8, u32>>,
    /// Number of successful reconnections since startup
    reconnect_count: AtomicU32,
    /// Timestamp of last disconnection
    last_disconnect: Mutex<Option<Instant>>,
    /// Lock to prevent concurrent reconnection attempts
    reconnect_lock: Mutex<()>,
}

impl ConnectionManager {
    /// Create a new connection manager
    pub fn new(
        controller: DefaultFanController,
        config: ReconnectConfig,
        timeout_ms: u64,
        debug_uart: bool,
    ) -> Self {
        Self {
            controller: RwLock::new(Some(controller)),
            state: RwLock::new(ConnectionState::Connected),
            config,
            timeout_ms,
            debug_uart,
            pwm_cache: Mutex::new(HashMap::new()),
            reconnect_count: AtomicU32::new(0),
            last_disconnect: Mutex::new(None),
            reconnect_lock: Mutex::new(()),
        }
    }

    /// Execute an operation on the fan controller with automatic disconnect detection
    ///
    /// If the operation fails due to a disconnection, the manager will:
    /// 1. Cache the current PWM state
    /// 2. Update the connection state to Disconnected
    /// 3. Return a `DeviceDisconnected` error
    ///
    /// Subsequent calls will return `Reconnecting` or attempt lazy reconnection.
    ///
    /// # Usage
    ///
    /// The closure must return a boxed future. Use `Box::pin(async move { ... })`:
    ///
    /// ```ignore
    /// cm.with_controller(|ctrl| Box::pin(async move {
    ///     ctrl.get_all_fan_rpm().await
    /// })).await
    /// ```
    pub async fn with_controller<F, T>(&self, f: F) -> Result<T>
    where
        F: for<'a> FnOnce(
            &'a mut DefaultFanController,
        ) -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>,
    {
        // Check current state
        let state = *self.state.read().await;

        match state {
            ConnectionState::Reconnecting { .. } => {
                return Err(OpenFanError::Reconnecting);
            }
            ConnectionState::Disconnected => {
                // Attempt lazy reconnection if enabled
                if self.config.enabled {
                    self.try_reconnect().await?;
                } else {
                    return Err(OpenFanError::DeviceDisconnected(
                        "Device disconnected and reconnection is disabled".to_string(),
                    ));
                }
            }
            ConnectionState::Connected => {}
        }

        // Execute the operation
        let mut controller_guard = self.controller.write().await;
        if let Some(ref mut controller) = *controller_guard {
            match f(controller).await {
                Ok(result) => Ok(result),
                Err(e) if is_disconnect_error(&e) => {
                    // Device disconnected during operation
                    drop(controller_guard);
                    self.handle_disconnect().await;
                    Err(OpenFanError::DeviceDisconnected(e.to_string()))
                }
                Err(e) => Err(e),
            }
        } else {
            Err(OpenFanError::DeviceNotFound)
        }
    }

    /// Handle a device disconnection
    async fn handle_disconnect(&self) {
        let mut state = self.state.write().await;

        // Only handle if currently connected
        if *state == ConnectionState::Connected {
            warn!("Device disconnected, caching state for recovery");

            // Cache PWM values before marking as disconnected
            if let Some(ref controller) = *self.controller.read().await {
                let mut pwm_cache = self.pwm_cache.lock().await;
                *pwm_cache = controller.get_all_fan_pwm();
                debug!("Cached {} PWM values", pwm_cache.len());
            }

            // Update state
            *state = ConnectionState::Disconnected;
            *self.last_disconnect.lock().await = Some(Instant::now());
        }
    }

    /// Attempt to reconnect to the device with exponential backoff
    async fn try_reconnect(&self) -> Result<()> {
        // Acquire reconnect lock to prevent concurrent attempts
        let _lock = self.reconnect_lock.lock().await;

        // Double-check state under lock
        if *self.state.read().await == ConnectionState::Connected {
            return Ok(());
        }

        if !self.config.enabled {
            return Err(OpenFanError::DeviceDisconnected(
                "Reconnection is disabled".to_string(),
            ));
        }

        let mut attempt = 0u32;
        let mut delay = Duration::from_secs(self.config.initial_delay_secs);
        let max_delay = Duration::from_secs(self.config.max_delay_secs);

        loop {
            attempt += 1;

            // Update state
            {
                let mut state = self.state.write().await;
                *state = ConnectionState::Reconnecting { attempt };
            }

            info!(
                "Reconnection attempt {}/{} (delay: {:?})",
                attempt,
                if self.config.max_attempts == 0 {
                    "unlimited".to_string()
                } else {
                    self.config.max_attempts.to_string()
                },
                delay
            );

            // Try to connect
            match connection::auto_connect(self.timeout_ms, self.debug_uart).await {
                Ok(mut new_controller) => {
                    // Verify connection works
                    if connection::test_connection(&mut new_controller)
                        .await
                        .is_ok()
                    {
                        info!("Reconnection successful after {} attempts", attempt);

                        // Restore PWM cache
                        let pwm_cache = self.pwm_cache.lock().await;
                        for (&fan_id, &pwm) in pwm_cache.iter() {
                            if let Err(e) = new_controller.set_fan_pwm(fan_id, pwm).await {
                                warn!("Failed to restore PWM for fan {}: {}", fan_id, e);
                            } else {
                                debug!("Restored PWM for fan {} to {}", fan_id, pwm);
                            }
                        }

                        // Update controller and state
                        *self.controller.write().await = Some(new_controller);
                        *self.state.write().await = ConnectionState::Connected;
                        self.reconnect_count.fetch_add(1, Ordering::Relaxed);

                        return Ok(());
                    } else {
                        debug!("Connection test failed, retrying...");
                    }
                }
                Err(e) => {
                    debug!("Reconnection attempt {} failed: {}", attempt, e);
                }
            }

            // Check max attempts
            if self.config.max_attempts > 0 && attempt >= self.config.max_attempts {
                error!("Reconnection failed after {} attempts, giving up", attempt);
                *self.state.write().await = ConnectionState::Disconnected;
                return Err(OpenFanError::ReconnectionFailed {
                    attempts: attempt,
                    reason: "Maximum reconnection attempts exceeded".to_string(),
                });
            }

            // Wait with exponential backoff
            sleep(delay).await;
            delay = Duration::from_secs_f64(
                (delay.as_secs_f64() * self.config.backoff_multiplier).min(max_delay.as_secs_f64()),
            );
        }
    }

    /// Get the current connection state
    pub async fn connection_state(&self) -> ConnectionState {
        *self.state.read().await
    }

    /// Get the number of successful reconnections since startup
    pub fn reconnect_count(&self) -> u32 {
        self.reconnect_count.load(Ordering::Relaxed)
    }

    /// Get the time since last disconnection (if any)
    pub async fn time_since_disconnect(&self) -> Option<Duration> {
        self.last_disconnect
            .lock()
            .await
            .map(|instant| instant.elapsed())
    }

    /// Check if reconnection is enabled
    pub fn reconnection_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Start a background heartbeat task that monitors connection health
    ///
    /// The heartbeat periodically checks the connection by querying firmware info.
    /// If the check fails with a disconnect error, it triggers the reconnection flow.
    pub fn start_heartbeat(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let interval = Duration::from_secs(self.config.heartbeat_interval_secs);

        tokio::spawn(async move {
            info!(
                "Starting connection heartbeat with {}s interval",
                interval.as_secs()
            );

            loop {
                sleep(interval).await;

                // Skip if not connected
                if *self.state.read().await != ConnectionState::Connected {
                    debug!("Heartbeat skipped: not connected");
                    continue;
                }

                // Perform health check
                debug!("Performing heartbeat check");
                let result = self
                    .with_controller(|ctrl| Box::pin(async move { ctrl.get_fw_info().await }))
                    .await;

                match result {
                    Ok(_) => {
                        debug!("Heartbeat successful");
                    }
                    Err(e) => {
                        warn!("Heartbeat failed: {}", e);
                        // Note: handle_disconnect is called by with_controller on disconnect
                    }
                }
            }
        })
    }

    /// Force a manual reconnection attempt
    ///
    /// This can be called by an API endpoint to trigger immediate reconnection.
    pub async fn force_reconnect(&self) -> Result<()> {
        // Mark as disconnected first
        {
            let mut state = self.state.write().await;
            if *state == ConnectionState::Connected {
                // Clear the controller
                *self.controller.write().await = None;
            }
            *state = ConnectionState::Disconnected;
        }

        // Attempt reconnection
        self.try_reconnect().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_state_as_str() {
        assert_eq!(ConnectionState::Connected.as_str(), "connected");
        assert_eq!(ConnectionState::Disconnected.as_str(), "disconnected");
        assert_eq!(
            ConnectionState::Reconnecting { attempt: 3 }.as_str(),
            "reconnecting"
        );
    }

    #[test]
    fn test_reconnect_config_defaults() {
        let config = ReconnectConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_attempts, 0);
        assert_eq!(config.initial_delay_secs, 1);
        assert_eq!(config.max_delay_secs, 30);
    }
}

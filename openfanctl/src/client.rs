//! HTTP client for communicating with the OpenFAN server.

use anyhow::{Context, Result};
use openfan_core::{
    api::{
        AliasResponse, ApiResponse, FanRpmResponse, FanStatusResponse, InfoResponse,
        ProfileResponse,
    },
    types::FanProfile,
    BoardConfig, DefaultBoard,
};
use reqwest::{Client, Response, StatusCode};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::time::Duration;

/// HTTP client for communicating with the OpenFAN daemon's REST API.
///
/// This client handles all HTTP communication with the server, including:
/// - Automatic retries on connection failures
/// - Timeout handling
/// - JSON serialization/deserialization
/// - Error response processing
///
/// # Retry Logic
///
/// The client automatically retries requests that fail due to:
/// - Connection errors (network unreachable, connection refused)
/// - Timeout errors
/// - Generic request errors
///
/// Retries use exponential backoff, with the delay increasing on each attempt.
/// Client errors (4xx) and server errors (5xx) are not retried.
///
/// # Examples
///
/// ```no_run
/// use openfanctl::client::OpenFanClient;
/// use std::time::Duration;
///
/// # async fn example() -> anyhow::Result<()> {
/// let client = OpenFanClient::with_config(
///     "http://localhost:3000".to_string(),
///     10,  // timeout in seconds
///     3,   // max retries
///     Duration::from_millis(500),  // initial retry delay
/// )?;
///
/// let info = client.get_info().await?;
/// println!("Server version: {}", info.version);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct OpenFanClient {
    client: Client,
    base_url: String,
    max_retries: u32,
    retry_delay: Duration,
}

impl OpenFanClient {
    /// Create a new OpenFAN client with custom configuration.
    ///
    /// # Arguments
    ///
    /// * `server_url` - Base URL of the OpenFAN server (e.g., "http://localhost:3000")
    /// * `timeout_secs` - Request timeout in seconds
    /// * `max_retries` - Maximum number of retry attempts for failed requests
    /// * `retry_delay` - Initial delay between retries (uses exponential backoff)
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be created.
    pub fn with_config(
        server_url: String,
        timeout_secs: u64,
        max_retries: u32,
        retry_delay: Duration,
    ) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .user_agent("openfanctl/1.0.0")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            base_url: server_url.trim_end_matches('/').to_string(),
            max_retries,
            retry_delay,
        })
    }

    /// Process an HTTP response and extract the API data.
    ///
    /// Handle both successful responses and various error conditions,
    /// providing detailed error messages for debugging.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The HTTP status code indicates failure (4xx or 5xx)
    /// - The response body cannot be read
    /// - The JSON cannot be deserialized
    /// - The API returns an error response
    async fn handle_response<T: DeserializeOwned>(response: Response, endpoint: &str) -> Result<T> {
        let status = response.status();
        let text = response
            .text()
            .await
            .with_context(|| format!("Failed to read response body from {}", endpoint))?;

        if !status.is_success() {
            let error_msg = match status {
                StatusCode::NOT_FOUND => format!("Endpoint {} not found", endpoint),
                StatusCode::BAD_REQUEST => format!("Bad request to {}: {}", endpoint, text),
                StatusCode::UNAUTHORIZED => format!("Unauthorized access to {}", endpoint),
                StatusCode::FORBIDDEN => format!("Access forbidden to {}", endpoint),
                StatusCode::INTERNAL_SERVER_ERROR => {
                    format!("Server error at {}: {}", endpoint, text)
                }
                StatusCode::SERVICE_UNAVAILABLE => format!("Service unavailable at {}", endpoint),
                _ => format!("HTTP {} error at {}: {}", status, endpoint, text),
            };
            return Err(anyhow::anyhow!(error_msg));
        }

        let api_response: ApiResponse<T> = serde_json::from_str(&text)
            .with_context(|| format!("Failed to parse JSON response from {}", endpoint))?;

        match api_response {
            ApiResponse::Success { data } => Ok(data),
            ApiResponse::Error { error } => {
                Err(anyhow::anyhow!("Server error at {}: {}", endpoint, error))
            }
        }
    }

    /// Execute an HTTP request with automatic retry logic.
    ///
    /// Only retry on connection-related errors (connection failures, timeouts).
    /// Client errors (4xx) and server errors (5xx) are not retried.
    ///
    /// Uses exponential backoff: retry delay increases with each attempt
    /// (delay * (attempt + 1)).
    ///
    /// # Errors
    ///
    /// Returns an error if all retry attempts fail.
    async fn execute_with_retry<F, Fut, T>(&self, endpoint: &str, request_fn: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<Response, reqwest::Error>>,
        T: DeserializeOwned,
    {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            match request_fn().await {
                Ok(response) => {
                    return Self::handle_response(response, endpoint).await;
                }
                Err(e) => {
                    // Only retry on connection errors, not client errors
                    let should_retry = e.is_connect() || e.is_timeout() || e.is_request();
                    last_error = Some(e);

                    // Don't retry on the last attempt
                    if attempt < self.max_retries && should_retry {
                        tokio::time::sleep(self.retry_delay * (attempt + 1)).await;
                        continue;
                    } else {
                        break;
                    }
                }
            }
        }

        Err(anyhow::anyhow!(
            "Failed to reach {} after {} attempts: {}",
            endpoint,
            self.max_retries + 1,
            last_error.unwrap()
        ))
    }

    /// Retrieve system information from the server.
    ///
    /// # Returns
    ///
    /// Returns server version, hardware connection status, and mock mode status.
    pub async fn get_info(&self) -> Result<InfoResponse> {
        let url = format!("{}/api/v0/info", self.base_url);
        let endpoint = "info";

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Retrieve the current status of all fans.
    ///
    /// # Returns
    ///
    /// Returns PWM values and RPM readings for all fans in the system.
    pub async fn get_fan_status(&self) -> Result<FanStatusResponse> {
        let url = format!("{}/api/v0/fan/status", self.base_url);
        let endpoint = "fan/status";

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Retrieve fan status for all fans.
    ///
    /// Note: Despite the `fan_id` parameter, this currently returns status for all fans.
    /// The parameter is ignored but kept for API compatibility.
    ///
    /// # Arguments
    ///
    /// * `_fan_id` - Currently ignored, reserved for future use
    pub async fn get_fan_status_by_id(&self, _fan_id: u8) -> Result<FanStatusResponse> {
        let url = format!("{}/api/v0/fan/status", self.base_url);
        let endpoint = "fan/status";

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Retrieve the current RPM reading for a specific fan.
    ///
    /// # Arguments
    ///
    /// * `fan_id` - Fan identifier (0-9)
    ///
    /// # Errors
    ///
    /// Returns an error if the fan ID is invalid (>= 10).
    pub async fn get_fan_rpm(&self, fan_id: u8) -> Result<FanRpmResponse> {
        if fan_id as usize >= DefaultBoard::FAN_COUNT {
            return Err(anyhow::anyhow!(
                "Invalid fan ID: {}. Must be 0-{}",
                fan_id,
                DefaultBoard::FAN_COUNT - 1
            ));
        }

        let url = format!("{}/api/v0/fan/{}/rpm/get", self.base_url, fan_id);
        let endpoint = &format!("fan/{}/rpm/get", fan_id);

        let rpm: u32 = self
            .execute_with_retry(endpoint, || self.client.get(&url).send())
            .await?;

        Ok(FanRpmResponse { fan_id, rpm })
    }

    /// Set the PWM (Pulse Width Modulation) value for a specific fan.
    ///
    /// PWM controls the fan speed as a percentage (0-100%).
    ///
    /// # Arguments
    ///
    /// * `fan_id` - Fan identifier (0-9)
    /// * `pwm` - PWM percentage (0-100)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The fan ID is invalid (>= 10)
    /// - The PWM value is out of range (> 100)
    pub async fn set_fan_pwm(&self, fan_id: u8, pwm: u32) -> Result<()> {
        if fan_id as usize >= DefaultBoard::FAN_COUNT {
            return Err(anyhow::anyhow!(
                "Invalid fan ID: {}. Must be 0-{}",
                fan_id,
                DefaultBoard::FAN_COUNT - 1
            ));
        }
        if pwm > 100 {
            return Err(anyhow::anyhow!("Invalid PWM value: {}. Must be 0-100", pwm));
        }

        let url = format!("{}/api/v0/fan/{}/pwm?value={}", self.base_url, fan_id, pwm);
        let endpoint = &format!("fan/{}/pwm", fan_id);

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
            .map(|_: ()| ())
    }

    /// Set the target RPM (Revolutions Per Minute) for a specific fan.
    ///
    /// This controls the fan speed directly by RPM rather than PWM percentage.
    ///
    /// # Arguments
    ///
    /// * `fan_id` - Fan identifier (0-9)
    /// * `rpm` - Target RPM value (must be < 10000)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The fan ID is invalid (>= 10)
    /// - The RPM value is unreasonable (>= 10000)
    pub async fn set_fan_rpm(&self, fan_id: u8, rpm: u32) -> Result<()> {
        if fan_id as usize >= DefaultBoard::FAN_COUNT {
            return Err(anyhow::anyhow!(
                "Invalid fan ID: {}. Must be 0-{}",
                fan_id,
                DefaultBoard::FAN_COUNT - 1
            ));
        }
        if rpm > 10000 {
            return Err(anyhow::anyhow!(
                "Invalid RPM value: {}. Must be reasonable (< 10000)",
                rpm
            ));
        }

        let url = format!("{}/api/v0/fan/{}/rpm?value={}", self.base_url, fan_id, rpm);
        let endpoint = &format!("fan/{}/rpm", fan_id);

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
            .map(|_: ()| ())
    }

    /// Retrieve all saved fan profiles from the server.
    ///
    /// # Returns
    ///
    /// Returns a list of profile names and their configurations.
    pub async fn get_profiles(&self) -> Result<ProfileResponse> {
        let url = format!("{}/api/v0/profiles/list", self.base_url);
        let endpoint = "profiles/list";

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Apply a saved fan profile by name.
    ///
    /// Set all fans according to the profile's configuration.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the profile to apply
    ///
    /// # Errors
    ///
    /// Returns an error if the profile name is empty or whitespace.
    pub async fn apply_profile(&self, name: &str) -> Result<()> {
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Profile name cannot be empty"));
        }

        let encoded_name = name.replace(' ', "%20").replace('&', "%26");
        let url = format!(
            "{}/api/v0/profiles/set?name={}",
            self.base_url, encoded_name
        );
        let endpoint = "profiles/set";

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
            .map(|_: ()| ())
    }

    /// Create a new fan profile with the given name and configuration.
    ///
    /// # Arguments
    ///
    /// * `name` - Name for the new profile
    /// * `profile` - Fan profile configuration (must have exactly 10 values)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The profile name is empty or whitespace
    /// - The profile doesn't have exactly 10 values (one per fan)
    pub async fn add_profile(&self, name: &str, profile: FanProfile) -> Result<()> {
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Profile name cannot be empty"));
        }
        if profile.values.len() != DefaultBoard::FAN_COUNT {
            return Err(anyhow::anyhow!(
                "Profile must have exactly {} values, got {}",
                DefaultBoard::FAN_COUNT,
                profile.values.len()
            ));
        }

        let url = format!("{}/api/v0/profiles/add", self.base_url);
        let mut request_body = HashMap::new();
        request_body.insert("name", serde_json::Value::String(name.to_string()));
        request_body.insert("profile", serde_json::to_value(profile)?);

        let endpoint = "profiles/add";

        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .with_context(|| format!("Failed to send add profile request to {}", endpoint))?;

        Self::handle_response(response, endpoint)
            .await
            .map(|_: ()| ())
    }

    /// Delete a saved fan profile by name.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the profile to remove
    ///
    /// # Errors
    ///
    /// Returns an error if the profile name is empty or whitespace.
    pub async fn remove_profile(&self, name: &str) -> Result<()> {
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Profile name cannot be empty"));
        }

        let encoded_name = name.replace(' ', "%20").replace('&', "%26");
        let url = format!(
            "{}/api/v0/profiles/remove?name={}",
            self.base_url, encoded_name
        );
        let endpoint = "profiles/remove";

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
            .map(|_: ()| ())
    }

    /// Retrieve all configured fan aliases.
    ///
    /// # Returns
    ///
    /// Returns a map of fan IDs to their human-readable alias names.
    pub async fn get_aliases(&self) -> Result<AliasResponse> {
        let url = format!("{}/api/v0/alias/all/get", self.base_url);
        let endpoint = "alias/all/get";

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Retrieve the alias for a specific fan.
    ///
    /// # Arguments
    ///
    /// * `fan_id` - Fan identifier (0-9)
    ///
    /// # Errors
    ///
    /// Returns an error if the fan ID is invalid (>= 10).
    pub async fn get_alias(&self, fan_id: u8) -> Result<AliasResponse> {
        if fan_id as usize >= DefaultBoard::FAN_COUNT {
            return Err(anyhow::anyhow!(
                "Invalid fan ID: {}. Must be 0-{}",
                fan_id,
                DefaultBoard::FAN_COUNT - 1
            ));
        }

        let url = format!("{}/api/v0/alias/{}/get", self.base_url, fan_id);
        let endpoint = &format!("alias/{}/get", fan_id);

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Set a human-readable alias for a specific fan.
    ///
    /// # Arguments
    ///
    /// * `fan_id` - Fan identifier (0-9)
    /// * `alias` - Human-readable name for the fan (max 100 characters)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The fan ID is invalid (>= 10)
    /// - The alias is empty or whitespace
    /// - The alias exceeds 100 characters
    pub async fn set_alias(&self, fan_id: u8, alias: &str) -> Result<()> {
        if fan_id as usize >= DefaultBoard::FAN_COUNT {
            return Err(anyhow::anyhow!(
                "Invalid fan ID: {}. Must be 0-{}",
                fan_id,
                DefaultBoard::FAN_COUNT - 1
            ));
        }
        if alias.trim().is_empty() {
            return Err(anyhow::anyhow!("Alias cannot be empty"));
        }
        if alias.len() > 100 {
            return Err(anyhow::anyhow!(
                "Alias too long: {} characters. Maximum 100",
                alias.len()
            ));
        }

        let encoded_alias = alias.replace(' ', "%20").replace('&', "%26");
        let url = format!(
            "{}/api/v0/alias/{}/set?value={}",
            self.base_url, fan_id, encoded_alias
        );
        let endpoint = &format!("alias/{}/set", fan_id);

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
            .map(|_: ()| ())
    }

    /// Test basic connectivity to the server.
    ///
    /// Use a short timeout (3 seconds) to quickly determine if the server is reachable.
    ///
    /// # Returns
    ///
    /// Returns `true` if the server responds with a success status, `false` otherwise.
    /// Does not return an error on connection failure - use for availability checks.
    pub async fn ping(&self) -> Result<bool> {
        let url = format!("{}/", self.base_url);

        // Use a shorter timeout for ping
        let client = Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .context("Failed to create ping client")?;

        match client.get(&url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(_e) => {
                // Return false for any ping failure (not an error)
                Ok(false)
            }
        }
    }

    /// Perform a comprehensive health check of the server connection.
    ///
    /// Test both basic connectivity and API functionality, providing detailed
    /// status information including response time and server capabilities.
    ///
    /// # Returns
    ///
    /// Returns a map containing:
    /// - `connected` - Whether the server is reachable
    /// - `ping_ms` - Response time in milliseconds
    /// - `api_working` - Whether the API endpoint responds correctly (if connected)
    /// - `server_version` - Server version string (if API is working)
    /// - `hardware_connected` - Whether hardware is connected (if API is working)
    /// - `api_error` - Error message if API check fails (if connected but API fails)
    pub async fn health_check(&self) -> Result<HashMap<String, serde_json::Value>> {
        let mut health = HashMap::new();

        // Test basic connectivity
        let ping_start = std::time::Instant::now();
        let ping_success = self.ping().await?;
        let ping_duration = ping_start.elapsed();

        health.insert(
            "connected".to_string(),
            serde_json::Value::Bool(ping_success),
        );
        health.insert(
            "ping_ms".to_string(),
            serde_json::Value::Number(serde_json::Number::from(ping_duration.as_millis() as u64)),
        );

        if ping_success {
            // Test API endpoint
            match self.get_info().await {
                Ok(info) => {
                    health.insert("api_working".to_string(), serde_json::Value::Bool(true));
                    health.insert(
                        "server_version".to_string(),
                        serde_json::Value::String(info.version),
                    );
                    health.insert(
                        "hardware_connected".to_string(),
                        serde_json::Value::Bool(info.hardware_connected),
                    );
                }
                Err(e) => {
                    health.insert("api_working".to_string(), serde_json::Value::Bool(false));
                    health.insert(
                        "api_error".to_string(),
                        serde_json::Value::String(e.to_string()),
                    );
                }
            }
        }

        Ok(health)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = OpenFanClient::with_config(
            "http://localhost:3000".to_string(),
            10,
            3,
            Duration::from_millis(500),
        );
        assert!(client.is_ok());

        let client = client.unwrap();
        assert_eq!(client.base_url, "http://localhost:3000");
    }

    #[test]
    fn test_url_trimming() {
        let client = OpenFanClient::with_config(
            "http://localhost:3000/".to_string(),
            10,
            3,
            Duration::from_millis(500),
        )
        .unwrap();
        assert_eq!(client.base_url, "http://localhost:3000");
    }

    #[tokio::test]
    async fn test_ping_unreachable_server() {
        let client = OpenFanClient::with_config(
            "http://localhost:9999".to_string(),
            10,
            3,
            Duration::from_millis(500),
        )
        .unwrap();
        let result = client.ping().await.unwrap();
        assert!(!result);
    }
}

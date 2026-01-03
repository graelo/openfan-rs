//! HTTP client for communicating with the OpenFAN server.

use anyhow::{Context, Result};
use openfan_core::{api, types::FanProfile, BoardInfo, CurvePoint};
use reqwest::{Client, Response, StatusCode};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::time::Duration;

/// Normalize a server URL by removing trailing slashes.
fn normalize_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

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
/// ).await?;
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
    board_info: BoardInfo,
}

impl OpenFanClient {
    /// Get the board information for the connected server.
    ///
    /// # Returns
    ///
    /// Returns the board info fetched during client initialization.
    pub fn board_info(&self) -> &BoardInfo {
        &self.board_info
    }

    /// Create a new OpenFAN client with custom configuration.
    ///
    /// Fetches board information from the server during initialization to ensure
    /// all subsequent operations are validated against the correct board type.
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
    /// Returns an error if:
    /// - The HTTP client cannot be created
    /// - The server is unreachable
    /// - Board information cannot be fetched from the server
    pub async fn with_config(
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

        let base_url = normalize_url(&server_url);

        // Create a temporary client to fetch board info
        let temp_client = Self {
            client: client.clone(),
            base_url: base_url.clone(),
            max_retries,
            retry_delay,
            board_info: BoardInfo {
                board_type: openfan_core::BoardType::OpenFanStandard,
                name: "Unknown".to_string(),
                fan_count: 10,
                max_pwm: 100,
                min_target_rpm: 500,
                max_target_rpm: 9000,
                baud_rate: 115200,
            },
        };

        // Fetch board info from server
        let info = temp_client
            .get_info()
            .await
            .context("Failed to fetch board information from server")?;

        Ok(Self {
            client,
            base_url,
            max_retries,
            retry_delay,
            board_info: info.board_info,
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

        let api_response: api::ApiResponse<T> = serde_json::from_str(&text)
            .with_context(|| format!("Failed to parse JSON response from {}", endpoint))?;

        match api_response {
            api::ApiResponse::Success { data } => Ok(data),
            api::ApiResponse::Error { error } => {
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
    pub async fn get_info(&self) -> Result<api::InfoResponse> {
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
    pub async fn get_fan_status(&self) -> Result<api::FanStatusResponse> {
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
    pub async fn get_fan_status_by_id(&self, _fan_id: u8) -> Result<api::FanStatusResponse> {
        let url = format!("{}/api/v0/fan/status", self.base_url);
        let endpoint = "fan/status";

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Retrieve the current RPM reading for a specific fan.
    ///
    /// # Arguments
    ///
    /// * `fan_id` - Fan identifier
    ///
    /// # Errors
    ///
    /// Returns an error if the fan ID is invalid for this board type.
    pub async fn get_fan_rpm(&self, fan_id: u8) -> Result<api::FanRpmResponse> {
        self.board_info.validate_fan_id(fan_id)?;

        let url = format!("{}/api/v0/fan/{}/rpm/get", self.base_url, fan_id);
        let endpoint = &format!("fan/{}/rpm/get", fan_id);

        let rpm: u32 = self
            .execute_with_retry(endpoint, || self.client.get(&url).send())
            .await?;

        Ok(api::FanRpmResponse { fan_id, rpm })
    }

    /// Set the PWM (Pulse Width Modulation) value for a specific fan.
    ///
    /// PWM controls the fan speed as a percentage.
    ///
    /// # Arguments
    ///
    /// * `fan_id` - Fan identifier
    /// * `pwm` - PWM percentage
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The fan ID is invalid for this board type
    /// - The PWM value is out of range for this board type
    pub async fn set_fan_pwm(&self, fan_id: u8, pwm: u32) -> Result<()> {
        self.board_info.validate_fan_id(fan_id)?;
        self.board_info.validate_pwm(pwm)?;

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
    /// * `fan_id` - Fan identifier
    /// * `rpm` - Target RPM value
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The fan ID is invalid for this board type
    /// - The RPM value is out of range for this board type
    pub async fn set_fan_rpm(&self, fan_id: u8, rpm: u32) -> Result<()> {
        self.board_info.validate_fan_id(fan_id)?;
        self.board_info.validate_target_rpm(rpm)?;

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
    pub async fn get_profiles(&self) -> Result<api::ProfileResponse> {
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
    /// * `profile` - Fan profile configuration
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The profile name is empty or whitespace
    /// - The profile doesn't have the correct number of values for this board type
    pub async fn add_profile(&self, name: &str, profile: FanProfile) -> Result<()> {
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Profile name cannot be empty"));
        }
        if profile.values.len() != self.board_info.fan_count {
            return Err(anyhow::anyhow!(
                "Profile must have exactly {} values for board '{}', got {}",
                self.board_info.fan_count,
                self.board_info.name,
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
    pub async fn get_aliases(&self) -> Result<api::AliasResponse> {
        let url = format!("{}/api/v0/alias/all/get", self.base_url);
        let endpoint = "alias/all/get";

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Retrieve the alias for a specific fan.
    ///
    /// # Arguments
    ///
    /// * `fan_id` - Fan identifier
    ///
    /// # Errors
    ///
    /// Returns an error if the fan ID is invalid for this board type.
    pub async fn get_alias(&self, fan_id: u8) -> Result<api::AliasResponse> {
        self.board_info.validate_fan_id(fan_id)?;

        let url = format!("{}/api/v0/alias/{}/get", self.base_url, fan_id);
        let endpoint = &format!("alias/{}/get", fan_id);

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Set a human-readable alias for a specific fan.
    ///
    /// # Arguments
    ///
    /// * `fan_id` - Fan identifier
    /// * `alias` - Human-readable name for the fan (max 100 characters)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The fan ID is invalid for this board type
    /// - The alias is empty or whitespace
    /// - The alias exceeds 100 characters
    pub async fn set_alias(&self, fan_id: u8, alias: &str) -> Result<()> {
        self.board_info.validate_fan_id(fan_id)?;
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

    /// Delete the alias for a specific fan (reverts to default).
    ///
    /// After deletion, the fan will display its default alias "Fan #N".
    ///
    /// # Arguments
    ///
    /// * `fan_id` - Fan identifier
    ///
    /// # Errors
    ///
    /// Returns an error if the fan ID is invalid for this board type.
    pub async fn delete_alias(&self, fan_id: u8) -> Result<()> {
        self.board_info.validate_fan_id(fan_id)?;

        let url = format!("{}/api/v0/alias/{}", self.base_url, fan_id);
        let endpoint = &format!("alias/{}", fan_id);

        self.execute_with_retry(endpoint, || self.client.delete(&url).send())
            .await
            .map(|_: ()| ())
    }

    // =========================================================================
    // Zone operations
    // =========================================================================

    /// Retrieve all configured zones.
    ///
    /// # Returns
    ///
    /// Returns a map of zone names to their configurations.
    pub async fn get_zones(&self) -> Result<api::ZoneResponse> {
        let url = format!("{}/api/v0/zones/list", self.base_url);
        let endpoint = "zones/list";

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Retrieve a specific zone by name.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the zone
    ///
    /// # Errors
    ///
    /// Returns an error if the zone name is empty or whitespace.
    pub async fn get_zone(&self, name: &str) -> Result<api::SingleZoneResponse> {
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Zone name cannot be empty"));
        }

        let encoded_name = name.replace(' ', "%20").replace('&', "%26");
        let url = format!("{}/api/v0/zone/{}/get", self.base_url, encoded_name);
        let endpoint = &format!("zone/{}/get", name);

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Create a new zone.
    ///
    /// # Arguments
    ///
    /// * `name` - Name for the new zone
    /// * `fans` - Fan references (controller + fan_id) to include in the zone
    /// * `description` - Optional description
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The zone name is empty or whitespace
    /// - Any fan ID is invalid for this board type
    pub async fn add_zone(
        &self,
        name: &str,
        fans: Vec<openfan_core::ZoneFan>,
        description: Option<String>,
    ) -> Result<()> {
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Zone name cannot be empty"));
        }

        // Validate fan IDs
        // TODO: In multi-controller mode, validate against each controller's board info
        for fan in &fans {
            self.board_info.validate_fan_id(fan.fan_id)?;
        }

        let url = format!("{}/api/v0/zones/add", self.base_url);
        let mut request_body = HashMap::new();
        request_body.insert("name", serde_json::Value::String(name.to_string()));
        request_body.insert("fans", serde_json::to_value(&fans)?);
        if let Some(desc) = description {
            request_body.insert("description", serde_json::Value::String(desc));
        }

        let endpoint = "zones/add";

        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .with_context(|| format!("Failed to send add zone request to {}", endpoint))?;

        Self::handle_response(response, endpoint)
            .await
            .map(|_: ()| ())
    }

    /// Update an existing zone.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the zone to update
    /// * `fans` - New fan references (controller + fan_id) for the zone
    /// * `description` - Optional new description
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The zone name is empty or whitespace
    /// - Any fan ID is invalid for this board type
    pub async fn update_zone(
        &self,
        name: &str,
        fans: Vec<openfan_core::ZoneFan>,
        description: Option<String>,
    ) -> Result<()> {
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Zone name cannot be empty"));
        }

        // Validate fan IDs
        // TODO: In multi-controller mode, validate against each controller's board info
        for fan in &fans {
            self.board_info.validate_fan_id(fan.fan_id)?;
        }

        let encoded_name = name.replace(' ', "%20").replace('&', "%26");
        let url = format!("{}/api/v0/zone/{}/update", self.base_url, encoded_name);
        let mut request_body = HashMap::new();
        request_body.insert("fans", serde_json::to_value(&fans)?);
        if let Some(desc) = description {
            request_body.insert("description", serde_json::Value::String(desc));
        }

        let endpoint = &format!("zone/{}/update", name);

        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .with_context(|| format!("Failed to send update zone request to {}", endpoint))?;

        Self::handle_response(response, endpoint)
            .await
            .map(|_: ()| ())
    }

    /// Delete a zone by name.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the zone to delete
    ///
    /// # Errors
    ///
    /// Returns an error if the zone name is empty or whitespace.
    pub async fn delete_zone(&self, name: &str) -> Result<()> {
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Zone name cannot be empty"));
        }

        let encoded_name = name.replace(' ', "%20").replace('&', "%26");
        let url = format!("{}/api/v0/zone/{}/delete", self.base_url, encoded_name);
        let endpoint = &format!("zone/{}/delete", name);

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
            .map(|_: ()| ())
    }

    /// Apply a PWM or RPM value to all fans in a zone.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the zone
    /// * `mode` - Control mode ("pwm" or "rpm")
    /// * `value` - Control value (0-100 for PWM, 0-16000 for RPM)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The zone name is empty or whitespace
    /// - The mode is invalid
    /// - The value is out of range for the specified mode
    pub async fn apply_zone(&self, name: &str, mode: &str, value: u16) -> Result<()> {
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Zone name cannot be empty"));
        }

        // Validate mode
        match mode.to_lowercase().as_str() {
            "pwm" => {
                if value > 100 {
                    return Err(anyhow::anyhow!(
                        "PWM value {} exceeds maximum of 100",
                        value
                    ));
                }
            }
            "rpm" => {
                if value > 16000 {
                    return Err(anyhow::anyhow!(
                        "RPM value {} exceeds maximum of 16000",
                        value
                    ));
                }
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "Invalid mode '{}'. Must be 'pwm' or 'rpm'.",
                    mode
                ));
            }
        }

        let encoded_name = name.replace(' ', "%20").replace('&', "%26");
        let url = format!(
            "{}/api/v0/zone/{}/apply?mode={}&value={}",
            self.base_url, encoded_name, mode, value
        );
        let endpoint = &format!("zone/{}/apply", name);

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
            .map(|_: ()| ())
    }

    // =========================================================================
    // Thermal curve operations
    // =========================================================================

    /// Retrieve all configured thermal curves.
    ///
    /// # Returns
    ///
    /// Returns a map of curve names to their configurations.
    pub async fn get_curves(&self) -> Result<api::ThermalCurveResponse> {
        let url = format!("{}/api/v0/curves/list", self.base_url);
        let endpoint = "curves/list";

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Retrieve a specific thermal curve by name.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the curve
    ///
    /// # Errors
    ///
    /// Returns an error if the curve name is empty or whitespace.
    pub async fn get_curve(&self, name: &str) -> Result<api::SingleCurveResponse> {
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Curve name cannot be empty"));
        }

        let encoded_name = name.replace(' ', "%20").replace('&', "%26");
        let url = format!("{}/api/v0/curve/{}/get", self.base_url, encoded_name);
        let endpoint = &format!("curve/{}/get", name);

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Create a new thermal curve.
    ///
    /// # Arguments
    ///
    /// * `name` - Name for the new curve
    /// * `points` - Temperature-to-PWM curve points
    /// * `description` - Optional description
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The curve name is empty or whitespace
    /// - The curve already exists
    /// - The points are invalid
    pub async fn add_curve(
        &self,
        name: &str,
        points: Vec<CurvePoint>,
        description: Option<String>,
    ) -> Result<()> {
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Curve name cannot be empty"));
        }

        let url = format!("{}/api/v0/curves/add", self.base_url);
        let mut request_body = HashMap::new();
        request_body.insert("name", serde_json::Value::String(name.to_string()));
        request_body.insert("points", serde_json::to_value(&points)?);
        if let Some(desc) = description {
            request_body.insert("description", serde_json::Value::String(desc));
        }

        let endpoint = "curves/add";

        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .with_context(|| format!("Failed to send add curve request to {}", endpoint))?;

        Self::handle_response(response, endpoint)
            .await
            .map(|_: ()| ())
    }

    /// Update an existing thermal curve.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the curve to update
    /// * `points` - New temperature-to-PWM curve points
    /// * `description` - Optional new description
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The curve name is empty or whitespace
    /// - The curve doesn't exist
    /// - The points are invalid
    pub async fn update_curve(
        &self,
        name: &str,
        points: Vec<CurvePoint>,
        description: Option<String>,
    ) -> Result<()> {
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Curve name cannot be empty"));
        }

        let encoded_name = name.replace(' ', "%20").replace('&', "%26");
        let url = format!("{}/api/v0/curve/{}/update", self.base_url, encoded_name);
        let mut request_body = HashMap::new();
        request_body.insert("points", serde_json::to_value(&points)?);
        if let Some(desc) = description {
            request_body.insert("description", serde_json::Value::String(desc));
        }

        let endpoint = &format!("curve/{}/update", name);

        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .with_context(|| format!("Failed to send update curve request to {}", endpoint))?;

        Self::handle_response(response, endpoint)
            .await
            .map(|_: ()| ())
    }

    /// Delete a thermal curve by name.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the curve to delete
    ///
    /// # Errors
    ///
    /// Returns an error if the curve name is empty or whitespace.
    pub async fn delete_curve(&self, name: &str) -> Result<()> {
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Curve name cannot be empty"));
        }

        let encoded_name = name.replace(' ', "%20").replace('&', "%26");
        let url = format!("{}/api/v0/curve/{}", self.base_url, encoded_name);
        let endpoint = &format!("curve/{}", name);

        self.execute_with_retry(endpoint, || self.client.delete(&url).send())
            .await
            .map(|_: ()| ())
    }

    /// Interpolate PWM value for a given temperature using a thermal curve.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the thermal curve
    /// * `temp` - Temperature in Celsius
    ///
    /// # Returns
    ///
    /// Returns the interpolated PWM value (0-100) for the given temperature.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The curve name is empty or whitespace
    /// - The curve doesn't exist
    pub async fn interpolate_curve(
        &self,
        name: &str,
        temp: f32,
    ) -> Result<api::InterpolateResponse> {
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Curve name cannot be empty"));
        }

        let encoded_name = name.replace(' ', "%20").replace('&', "%26");
        let url = format!(
            "{}/api/v0/curve/{}/interpolate?temp={}",
            self.base_url, encoded_name, temp
        );
        let endpoint = &format!("curve/{}/interpolate", name);

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    // =========================================================================
    // CFM mapping operations
    // =========================================================================

    /// Retrieve all configured CFM mappings.
    ///
    /// # Returns
    ///
    /// Returns a map of port IDs to their CFM@100% values.
    pub async fn get_cfm_mappings(&self) -> Result<api::CfmListResponse> {
        let url = format!("{}/api/v0/cfm/list", self.base_url);
        let endpoint = "cfm/list";

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Retrieve the CFM mapping for a specific port.
    ///
    /// # Arguments
    ///
    /// * `port` - Port identifier
    ///
    /// # Errors
    ///
    /// Returns an error if the port ID is invalid for this board type.
    pub async fn get_cfm(&self, port: u8) -> Result<api::CfmGetResponse> {
        self.board_info.validate_fan_id(port)?;

        let url = format!("{}/api/v0/cfm/{}", self.base_url, port);
        let endpoint = &format!("cfm/{}", port);

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Set the CFM@100% value for a specific port.
    ///
    /// # Arguments
    ///
    /// * `port` - Port identifier
    /// * `cfm_at_100` - CFM value when fan runs at 100% PWM
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The port ID is invalid for this board type
    /// - The CFM value is not positive
    /// - The CFM value exceeds the maximum allowed (500)
    pub async fn set_cfm(&self, port: u8, cfm_at_100: f32) -> Result<()> {
        self.board_info.validate_fan_id(port)?;

        if cfm_at_100 <= 0.0 {
            return Err(anyhow::anyhow!("CFM value must be positive"));
        }
        if cfm_at_100 > 500.0 {
            return Err(anyhow::anyhow!("CFM value must be <= 500"));
        }

        let url = format!("{}/api/v0/cfm/{}", self.base_url, port);
        let request = api::SetCfmRequest { cfm_at_100 };
        let endpoint = &format!("cfm/{}", port);

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .with_context(|| format!("Failed to send set CFM request to {}", endpoint))?;

        Self::handle_response(response, endpoint)
            .await
            .map(|_: ()| ())
    }

    /// Delete the CFM mapping for a specific port.
    ///
    /// # Arguments
    ///
    /// * `port` - Port identifier
    ///
    /// # Errors
    ///
    /// Returns an error if the port ID is invalid for this board type.
    pub async fn delete_cfm(&self, port: u8) -> Result<()> {
        self.board_info.validate_fan_id(port)?;

        let url = format!("{}/api/v0/cfm/{}", self.base_url, port);
        let endpoint = &format!("cfm/{}", port);

        self.execute_with_retry(endpoint, || self.client.delete(&url).send())
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

    // =========================================================================
    // Controller Management (Multi-Controller Support)
    // =========================================================================

    /// List all registered controllers.
    ///
    /// # Returns
    ///
    /// Returns information about all controllers registered with the server.
    pub async fn list_controllers(&self) -> Result<api::ControllersListResponse> {
        let url = format!("{}/api/v0/controllers", self.base_url);
        let endpoint = "controllers";

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Get information about a specific controller.
    ///
    /// # Arguments
    ///
    /// * `controller_id` - ID of the controller to query
    ///
    /// # Errors
    ///
    /// Returns an error if the controller does not exist.
    pub async fn get_controller_info(&self, controller_id: &str) -> Result<api::ControllerInfo> {
        let url = format!("{}/api/v0/controller/{}/info", self.base_url, controller_id);
        let endpoint = &format!("controller/{}/info", controller_id);

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Force a reconnection attempt for a specific controller.
    ///
    /// # Arguments
    ///
    /// * `controller_id` - ID of the controller to reconnect
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The controller does not exist
    /// - The controller is in mock mode
    pub async fn reconnect_controller(&self, controller_id: &str) -> Result<String> {
        let url = format!(
            "{}/api/v0/controller/{}/reconnect",
            self.base_url, controller_id
        );
        let endpoint = &format!("controller/{}/reconnect", controller_id);

        let response = self
            .client
            .post(&url)
            .send()
            .await
            .with_context(|| format!("Failed to send reconnect request to {}", endpoint))?;

        Self::handle_response(response, endpoint).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfan_core::BoardType;

    #[test]
    fn test_normalize_url() {
        assert_eq!(
            normalize_url("http://localhost:3000"),
            "http://localhost:3000"
        );
        assert_eq!(
            normalize_url("http://localhost:3000/"),
            "http://localhost:3000"
        );
        assert_eq!(
            normalize_url("http://localhost:3000///"),
            "http://localhost:3000"
        );
        assert_eq!(
            normalize_url("http://example.com/api/"),
            "http://example.com/api"
        );
    }

    #[test]
    fn test_board_info_validation() {
        let board_info = BoardType::OpenFanStandard.to_board_info();

        // Test valid fan ID
        assert!(board_info.validate_fan_id(0).is_ok());
        assert!(board_info.validate_fan_id(9).is_ok());

        // Test invalid fan ID
        assert!(board_info.validate_fan_id(10).is_err());
        assert!(board_info.validate_fan_id(255).is_err());
    }
}

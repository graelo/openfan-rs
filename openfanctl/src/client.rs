//! HTTP client for communicating with the OpenFAN server.

use anyhow::{Context, Result};
use openfan_core::{
    api::{
        AliasResponse, ApiResponse, FanRpmResponse, FanStatusResponse, InfoResponse,
        ProfileResponse,
    },
    types::FanProfile,
    MAX_FANS,
};
use reqwest::{Client, Response, StatusCode};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::time::Duration;

/// HTTP client for the OpenFAN server
#[derive(Debug, Clone)]
pub struct OpenFanClient {
    client: Client,
    base_url: String,
    max_retries: u32,
    retry_delay: Duration,
}

impl OpenFanClient {
    /// Create a new client with custom configuration
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

    /// Handle API response and extract data with enhanced error reporting
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

    /// Execute a request with retry logic
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

    /// Get system information
    pub async fn get_info(&self) -> Result<InfoResponse> {
        let url = format!("{}/api/v0/info", self.base_url);
        let endpoint = "info";

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Get fan status for all fans
    pub async fn get_fan_status(&self) -> Result<FanStatusResponse> {
        let url = format!("{}/api/v0/fan/status", self.base_url);
        let endpoint = "fan/status";

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Get fan status for a specific fan
    pub async fn get_fan_status_by_id(&self, _fan_id: u8) -> Result<FanStatusResponse> {
        let url = format!("{}/api/v0/fan/status", self.base_url);
        let endpoint = "fan/status";

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Get RPM for a specific fan
    pub async fn get_fan_rpm(&self, fan_id: u8) -> Result<FanRpmResponse> {
        if fan_id as usize >= MAX_FANS {
            return Err(anyhow::anyhow!(
                "Invalid fan ID: {}. Must be 0-{}",
                fan_id,
                MAX_FANS - 1
            ));
        }

        let url = format!("{}/api/v0/fan/{}/rpm/get", self.base_url, fan_id);
        let endpoint = &format!("fan/{}/rpm/get", fan_id);

        let rpm: u32 = self
            .execute_with_retry(endpoint, || self.client.get(&url).send())
            .await?;

        Ok(FanRpmResponse { fan_id, rpm })
    }

    /// Set fan PWM
    pub async fn set_fan_pwm(&self, fan_id: u8, pwm: u32) -> Result<()> {
        if fan_id as usize >= MAX_FANS {
            return Err(anyhow::anyhow!(
                "Invalid fan ID: {}. Must be 0-{}",
                fan_id,
                MAX_FANS - 1
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

    /// Set fan RPM
    pub async fn set_fan_rpm(&self, fan_id: u8, rpm: u32) -> Result<()> {
        if fan_id as usize >= MAX_FANS {
            return Err(anyhow::anyhow!(
                "Invalid fan ID: {}. Must be 0-{}",
                fan_id,
                MAX_FANS - 1
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

    /// Get all profiles
    pub async fn get_profiles(&self) -> Result<ProfileResponse> {
        let url = format!("{}/api/v0/profiles/list", self.base_url);
        let endpoint = "profiles/list";

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Apply a profile
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

    /// Add a new profile
    pub async fn add_profile(&self, name: &str, profile: FanProfile) -> Result<()> {
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Profile name cannot be empty"));
        }
        if profile.values.len() != MAX_FANS {
            return Err(anyhow::anyhow!(
                "Profile must have exactly {} values, got {}",
                MAX_FANS,
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

    /// Remove a profile
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

    /// Get all aliases
    pub async fn get_aliases(&self) -> Result<AliasResponse> {
        let url = format!("{}/api/v0/alias/all/get", self.base_url);
        let endpoint = "alias/all/get";

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Get alias for a specific fan
    pub async fn get_alias(&self, fan_id: u8) -> Result<AliasResponse> {
        if fan_id as usize >= MAX_FANS {
            return Err(anyhow::anyhow!(
                "Invalid fan ID: {}. Must be 0-{}",
                fan_id,
                MAX_FANS - 1
            ));
        }

        let url = format!("{}/api/v0/alias/{}/get", self.base_url, fan_id);
        let endpoint = &format!("alias/{}/get", fan_id);

        self.execute_with_retry(endpoint, || self.client.get(&url).send())
            .await
    }

    /// Set alias for a fan
    pub async fn set_alias(&self, fan_id: u8, alias: &str) -> Result<()> {
        if fan_id as usize >= MAX_FANS {
            return Err(anyhow::anyhow!(
                "Invalid fan ID: {}. Must be 0-{}",
                fan_id,
                MAX_FANS - 1
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

    /// Test server connectivity with detailed error reporting
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

    /// Get connection health with detailed status
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

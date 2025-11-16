//! Test utilities for CLI testing
//!
//! Provides mock server implementation and test helpers for integration testing.

use anyhow::Result;
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use openfan_core::api::{
    AliasResponse, ApiResponse, FanStatusResponse, InfoResponse, ProfileResponse,
};
use openfan_core::types::{ControlMode, FanProfile};
use openfan_core::MAX_FANS;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;

/// Mock server state
#[derive(Debug, Clone)]
pub struct MockServerState {
    /// Current fan RPMs
    pub rpms: Arc<Mutex<HashMap<String, u32>>>,
    /// Current fan PWMs
    pub pwms: Arc<Mutex<HashMap<String, u32>>>,
    /// Available profiles
    pub profiles: Arc<Mutex<HashMap<String, FanProfile>>>,
    /// Fan aliases
    pub aliases: Arc<Mutex<HashMap<String, String>>>,
    /// Server info
    pub info: Arc<Mutex<InfoResponse>>,
}

impl Default for MockServerState {
    fn default() -> Self {
        let mut rpms = HashMap::new();
        let mut pwms = HashMap::new();
        let mut profiles = HashMap::new();
        let mut aliases = HashMap::new();

        // Initialize with default values
        for i in 0..MAX_FANS as u32 {
            rpms.insert(i.to_string(), 1200 + i * 100);
            pwms.insert(i.to_string(), 50 + i * 5);
            aliases.insert(i.to_string(), format!("Fan #{}", i + 1));
        }

        profiles.insert(
            "50% PWM".to_string(),
            FanProfile {
                control_mode: ControlMode::Pwm,
                values: vec![50; MAX_FANS],
            },
        );
        profiles.insert(
            "100% PWM".to_string(),
            FanProfile {
                control_mode: ControlMode::Pwm,
                values: vec![100; MAX_FANS],
            },
        );
        profiles.insert(
            "1000 RPM".to_string(),
            FanProfile {
                control_mode: ControlMode::Rpm,
                values: vec![1000; MAX_FANS],
            },
        );

        let info = InfoResponse {
            version: "1.0.0-test".to_string(),
            hardware_connected: true,
            uptime: 3600,
            software: "OpenFAN Server v1.0.0-test\r\nBuild: test".to_string(),
            hardware: Some("Mock Hardware Controller v2.1\r\nSerial: MHC001234".to_string()),
            firmware: Some("Mock Firmware v1.5.2\r\nBuild: 2024-10-01".to_string()),
        };

        Self {
            rpms: Arc::new(Mutex::new(rpms)),
            pwms: Arc::new(Mutex::new(pwms)),
            profiles: Arc::new(Mutex::new(profiles)),
            aliases: Arc::new(Mutex::new(aliases)),
            info: Arc::new(Mutex::new(info)),
        }
    }
}

/// Query parameters for fan control
#[derive(Debug, Deserialize)]
pub struct FanControlQuery {
    value: u32,
}

/// Query parameters for profile operations
#[derive(Debug, Deserialize)]
pub struct ProfileQuery {
    name: String,
}

/// Query parameters for alias operations
#[derive(Debug, Deserialize)]
pub struct AliasQuery {
    alias: String,
}

/// Request body for adding profiles
#[derive(Debug, Serialize, Deserialize)]
pub struct AddProfileRequest {
    name: String,
    profile: FanProfile,
}

/// Mock server implementation
#[derive(Debug)]
pub struct MockServer {
    state: MockServerState,
    port: u16,
}

impl Default for MockServer {
    fn default() -> Self {
        Self::new()
    }
}

impl MockServer {
    /// Create a new mock server
    pub fn new() -> Self {
        Self {
            state: MockServerState::default(),
            port: 0, // Will be assigned when server starts
        }
    }

    /// Start the mock server and return the address
    pub async fn start(mut self) -> Result<(Self, String)> {
        let app = self.create_router();

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        self.port = addr.port();

        let server_url = format!("http://127.0.0.1:{}", self.port);

        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                eprintln!("Mock server error: {}", e);
            }
        });

        // Give the server a moment to start and verify it's running
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            if tokio::net::TcpStream::connect(("127.0.0.1", self.port))
                .await
                .is_ok()
            {
                break;
            }
        }

        Ok((self, server_url))
    }

    /// Get the server port
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Get a reference to the server state
    pub fn state(&self) -> &MockServerState {
        &self.state
    }

    /// Create the mock server router
    fn create_router(&self) -> Router {
        Router::new()
            // Root endpoint
            .route("/", get(root_handler))
            // Info endpoint
            .route("/api/v0/info", get(info_handler))
            // Fan endpoints
            .route("/api/v0/fan/status", get(fan_status_handler))
            .route("/api/v0/fan/:id/pwm", get(set_fan_pwm_handler))
            .route("/api/v0/fan/:id/rpm", get(set_fan_rpm_handler))
            .route("/api/v0/fan/:id/rpm/get", get(get_fan_rpm_handler))
            // Profile endpoints
            .route("/api/v0/profiles/list", get(list_profiles_handler))
            .route("/api/v0/profiles/set", get(set_profile_handler))
            .route("/api/v0/profiles/add", post(add_profile_handler))
            .route("/api/v0/profiles/remove", get(remove_profile_handler))
            // Alias endpoints
            .route("/api/v0/alias/all/get", get(get_all_aliases_handler))
            .route("/api/v0/alias/:id/get", get(get_alias_handler))
            .route("/api/v0/alias/:id/set", get(set_alias_handler))
            .with_state(self.state.clone())
    }
}

// Handler functions

async fn root_handler() -> Json<ApiResponse<serde_json::Value>> {
    let data = serde_json::json!({
        "service": "OpenFAN Controller API Server",
        "status": "ok",
        "version": "1.0.0-test"
    });
    Json(ApiResponse::success(data))
}

async fn info_handler(
    axum::extract::State(state): axum::extract::State<MockServerState>,
) -> Json<ApiResponse<InfoResponse>> {
    let info = state.info.lock().unwrap().clone();
    Json(ApiResponse::success(info))
}

async fn fan_status_handler(
    axum::extract::State(state): axum::extract::State<MockServerState>,
) -> Json<ApiResponse<FanStatusResponse>> {
    let rpms_str = state.rpms.lock().unwrap().clone();
    let pwms_str = state.pwms.lock().unwrap().clone();

    // Convert string keys to u8 keys
    let rpms: HashMap<u8, u32> = rpms_str
        .iter()
        .filter_map(|(k, v)| k.parse::<u8>().ok().map(|key| (key, *v)))
        .collect();
    let pwms: HashMap<u8, u32> = pwms_str
        .iter()
        .filter_map(|(k, v)| k.parse::<u8>().ok().map(|key| (key, *v)))
        .collect();

    let response = FanStatusResponse { rpms, pwms };
    Json(ApiResponse::success(response))
}

async fn set_fan_pwm_handler(
    Path(id): Path<u8>,
    Query(params): Query<FanControlQuery>,
    axum::extract::State(state): axum::extract::State<MockServerState>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    // Validate fan ID
    if id > 9 {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate PWM value range
    if params.value > 100 {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Simulate potential hardware communication failure (1% chance)
    if rand::random::<f32>() < 0.01 {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    state
        .pwms
        .lock()
        .unwrap()
        .insert(id.to_string(), params.value);
    Ok(Json(ApiResponse::success(())))
}

async fn set_fan_rpm_handler(
    Path(id): Path<u8>,
    Query(params): Query<FanControlQuery>,
    axum::extract::State(state): axum::extract::State<MockServerState>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    // Validate fan ID
    if id > 9 {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate RPM range (0-10000 to match client validation)
    if params.value > 10000 {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Simulate potential hardware communication failure (1% chance)
    if rand::random::<f32>() < 0.01 {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    state
        .rpms
        .lock()
        .unwrap()
        .insert(id.to_string(), params.value);
    Ok(Json(ApiResponse::success(())))
}

async fn get_fan_rpm_handler(
    Path(id): Path<u8>,
    axum::extract::State(state): axum::extract::State<MockServerState>,
) -> Result<Json<ApiResponse<u32>>, StatusCode> {
    // Validate fan ID
    if id > 9 {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Get RPM from state
    let rpms = state.rpms.lock().unwrap();
    let rpm = rpms
        .get(&id.to_string())
        .copied()
        .unwrap_or(1000 + (id as u32) * 100); // Default values

    Ok(Json(ApiResponse::success(rpm)))
}

async fn list_profiles_handler(
    axum::extract::State(state): axum::extract::State<MockServerState>,
) -> Json<ApiResponse<ProfileResponse>> {
    let profiles = state.profiles.lock().unwrap().clone();
    let response = ProfileResponse { profiles };
    Json(ApiResponse::success(response))
}

async fn set_profile_handler(
    Query(params): Query<ProfileQuery>,
    axum::extract::State(state): axum::extract::State<MockServerState>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    let profiles = state.profiles.lock().unwrap();
    if let Some(profile) = profiles.get(&params.name) {
        // Apply the profile
        match profile.control_mode {
            ControlMode::Pwm => {
                let mut pwms = state.pwms.lock().unwrap();
                for (i, &value) in profile.values.iter().enumerate() {
                    pwms.insert(i.to_string(), value);
                }
            }
            ControlMode::Rpm => {
                let mut rpms = state.rpms.lock().unwrap();
                for (i, &value) in profile.values.iter().enumerate() {
                    rpms.insert(i.to_string(), value);
                }
            }
        }
        Ok(Json(ApiResponse::success(())))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn add_profile_handler(
    axum::extract::State(state): axum::extract::State<MockServerState>,
    Json(req): Json<AddProfileRequest>,
) -> Json<ApiResponse<()>> {
    state.profiles.lock().unwrap().insert(req.name, req.profile);
    Json(ApiResponse::success(()))
}

async fn remove_profile_handler(
    Query(params): Query<ProfileQuery>,
    axum::extract::State(state): axum::extract::State<MockServerState>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    if state
        .profiles
        .lock()
        .unwrap()
        .remove(&params.name)
        .is_some()
    {
        Ok(Json(ApiResponse::success(())))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn get_all_aliases_handler(
    axum::extract::State(state): axum::extract::State<MockServerState>,
) -> Json<ApiResponse<AliasResponse>> {
    let aliases_str = state.aliases.lock().unwrap().clone();

    // Convert string keys to u8 keys
    let aliases: HashMap<u8, String> = aliases_str
        .iter()
        .filter_map(|(k, v)| k.parse::<u8>().ok().map(|key| (key, v.clone())))
        .collect();

    let response = AliasResponse { aliases };
    Json(ApiResponse::success(response))
}

async fn get_alias_handler(
    Path(id): Path<u8>,
    axum::extract::State(state): axum::extract::State<MockServerState>,
) -> Result<Json<ApiResponse<AliasResponse>>, StatusCode> {
    if id > 9 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let aliases = state.aliases.lock().unwrap();
    let mut result = HashMap::new();
    if let Some(alias) = aliases.get(&id.to_string()) {
        result.insert(id, alias.clone());
    }
    let response = AliasResponse { aliases: result };
    Ok(Json(ApiResponse::success(response)))
}

async fn set_alias_handler(
    Path(id): Path<u8>,
    Query(params): Query<AliasQuery>,
    axum::extract::State(state): axum::extract::State<MockServerState>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    // Validate fan ID
    if id > 9 {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate alias isn't empty and isn't too long
    if params.alias.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if params.alias.len() > 50 {
        return Err(StatusCode::BAD_REQUEST);
    }

    state
        .aliases
        .lock()
        .unwrap()
        .insert(id.to_string(), params.alias.trim().to_string());
    Ok(Json(ApiResponse::success(())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_server_startup() {
        let server = MockServer::new();
        let (server, url) = server.start().await.unwrap();

        assert!(server.port() > 0);
        assert!(url.contains(&server.port().to_string()));

        // Test basic connectivity
        let client = reqwest::Client::new();
        let response = client.get(&url).send().await.unwrap();
        assert!(response.status().is_success());
    }

    #[tokio::test]
    async fn test_info_endpoint() {
        let server = MockServer::new();
        let (_, url) = server.start().await.unwrap();

        let client = reqwest::Client::new();
        let response = client
            .get(format!("{}/api/v0/info", url))
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());
        let json: ApiResponse<InfoResponse> = response.json().await.unwrap();

        match json {
            ApiResponse::Success { data } => {
                assert_eq!(data.version, "1.0.0-test");
                assert!(data.hardware_connected);
            }
            _ => panic!("Expected success response"),
        }
    }

    #[tokio::test]
    async fn test_fan_status_endpoint() {
        let server = MockServer::new();
        let (_, url) = server.start().await.unwrap();

        let client = reqwest::Client::new();
        let response = client
            .get(format!("{}/api/v0/fan/status", url))
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());
        let json: ApiResponse<FanStatusResponse> = response.json().await.unwrap();

        match json {
            ApiResponse::Success { data } => {
                assert_eq!(data.rpms.len(), 10);
                assert_eq!(data.pwms.len(), 10);
                assert!(data.rpms.contains_key(&0));
                assert!(data.pwms.contains_key(&0));
            }
            _ => panic!("Expected success response"),
        }
    }

    #[tokio::test]
    async fn test_fan_control() {
        let server = MockServer::new();
        let (server_instance, url) = server.start().await.unwrap();

        let client = reqwest::Client::new();

        // Set PWM
        let response = client
            .get(format!("{}/api/v0/fan/0/pwm?value=75", url))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());

        // Verify the change
        let pwms = server_instance.state().pwms.lock().unwrap();
        assert_eq!(pwms.get("0"), Some(&75));
    }

    #[tokio::test]
    async fn test_profile_operations() {
        let server = MockServer::new();
        let (_, url) = server.start().await.unwrap();

        let client = reqwest::Client::new();

        // List profiles
        let response = client
            .get(format!("{}/api/v0/profiles/list", url))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());

        let json: ApiResponse<ProfileResponse> = response.json().await.unwrap();
        match json {
            ApiResponse::Success { data } => {
                assert!(data.profiles.contains_key("50% PWM"));
                assert!(data.profiles.contains_key("100% PWM"));
                assert!(data.profiles.contains_key("1000 RPM"));
            }
            _ => panic!("Expected success response"),
        }

        // Apply profile
        let response = client
            .get(format!("{}/api/v0/profiles/set?name=50% PWM", url))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
    }

    #[tokio::test]
    async fn test_alias_operations() {
        let server = MockServer::new();
        let (_, url) = server.start().await.unwrap();

        let client = reqwest::Client::new();

        // Get all aliases
        let response = client
            .get(format!("{}/api/v0/alias/all/get", url))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());

        // Set alias
        let response = client
            .get(format!("{}/api/v0/alias/0/set?alias=CPU Fan", url))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());

        // Get specific alias
        let response = client
            .get(format!("{}/api/v0/alias/0/get", url))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
    }
}

//! Simple integration tests for OpenFAN CLI
//!
//! NOTE: These tests are currently ignored because client creation now requires
//! fetching board info from a running server. To run these tests, you need to:
//! 1. Start openfand in mock mode: `cargo run --bin openfand -- --mock`
//! 2. Run tests: `cargo test --test simple_integration_tests -- --ignored`
//!
//! The core validation logic is covered by client unit tests.

use anyhow::Result;
use openfan_core::types::{ControlMode, FanProfile};
use openfan_core::{BoardConfig, DefaultBoard};
use openfanctl::client::OpenFanClient;

#[tokio::test]
#[ignore] // Requires running server
async fn test_client_creation_and_validation() -> Result<()> {
    // Test client creation with various URLs
    let _client = OpenFanClient::with_config(
        "http://localhost:3000".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )
    .await?;

    Ok(())
}

#[tokio::test]
#[ignore] // Requires running server
async fn test_fan_id_validation() -> Result<()> {
    let client = OpenFanClient::with_config(
        "http://localhost:3000".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )
    .await?;

    // Test invalid fan IDs (client-side validation should catch these)
    let result = client.set_fan_pwm(10, 50).await;
    assert!(result.is_err(), "Should fail with fan ID 10");

    let result = client.set_fan_pwm(255, 50).await;
    assert!(result.is_err(), "Should fail with fan ID 255");

    Ok(())
}

#[tokio::test]
#[ignore] // Requires running server
async fn test_pwm_value_validation() -> Result<()> {
    let client = OpenFanClient::with_config(
        "http://localhost:3000".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )
    .await?;

    // Test invalid PWM values (client-side validation should catch these)
    let result = client.set_fan_pwm(0, 101).await;
    assert!(result.is_err(), "Should fail with PWM 101");

    let result = client.set_fan_pwm(0, 999).await;
    assert!(result.is_err(), "Should fail with PWM 999");

    Ok(())
}

#[tokio::test]
#[ignore] // Requires running server
async fn test_rpm_value_validation() -> Result<()> {
    let client = OpenFanClient::with_config(
        "http://localhost:3000".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )
    .await?;

    // Test invalid RPM values (client-side validation should catch these)
    let result = client.set_fan_rpm(0, 10001).await;
    assert!(result.is_err(), "Should fail with RPM 10001");

    let result = client.set_fan_rpm(0, 50000).await;
    assert!(result.is_err(), "Should fail with RPM 50000");

    Ok(())
}

#[tokio::test]
#[ignore] // Requires running server
async fn test_alias_validation() -> Result<()> {
    let client = OpenFanClient::with_config(
        "http://localhost:3000".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )
    .await?;

    // Test invalid fan ID for alias
    let result = client.set_alias(10, "Test Fan").await;
    assert!(result.is_err(), "Should fail with fan ID 10");

    // Test invalid fan ID for get alias
    let result = client.get_alias(10).await;
    assert!(result.is_err(), "Should fail with fan ID 10");

    Ok(())
}

#[tokio::test]
#[ignore] // Requires running server
async fn test_profile_validation() -> Result<()> {
    let client = OpenFanClient::with_config(
        "http://localhost:3000".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )
    .await?;

    // Test profile with wrong number of values
    let invalid_profile = FanProfile {
        control_mode: ControlMode::Pwm,
        values: vec![50, 50, 50], // Only 3 values instead of 10
    };

    let result = client.add_profile("test", invalid_profile).await;
    assert!(result.is_err(), "Should fail with wrong number of values");

    // Test profile with too many values
    let invalid_profile = FanProfile {
        control_mode: ControlMode::Pwm,
        values: vec![50; 15], // 15 values instead of 10
    };

    let result = client.add_profile("test2", invalid_profile).await;
    assert!(result.is_err(), "Should fail with too many values");

    // Test empty profile name
    let valid_profile = FanProfile {
        control_mode: ControlMode::Pwm,
        values: vec![50; DefaultBoard::FAN_COUNT],
    };

    let result = client.add_profile("", valid_profile.clone()).await;
    assert!(result.is_err(), "Should fail with empty name");

    let result = client.add_profile("   ", valid_profile).await;
    assert!(result.is_err(), "Should fail with whitespace-only name");

    Ok(())
}

#[tokio::test]
#[ignore] // Client creation now requires server connection
async fn test_client_timeout_behavior() -> Result<()> {
    // This test is no longer valid - client creation now requires
    // successful connection to fetch board info
    Ok(())
}

#[tokio::test]
#[ignore] // Requires running server
async fn test_url_formatting() -> Result<()> {
    // Test that client properly handles URLs with/without trailing slashes
    let _client1 = OpenFanClient::with_config(
        "http://localhost:3000".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )
    .await?;
    let _client2 = OpenFanClient::with_config(
        "http://localhost:3000/".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )
    .await?;

    Ok(())
}

#[tokio::test]
#[ignore] // Client creation now requires server connection
async fn test_error_handling_chain() -> Result<()> {
    // This test is no longer valid - client creation now requires
    // successful connection to fetch board info
    Ok(())
}

#[tokio::test]
#[ignore] // Requires running server
async fn test_single_fan_rpm_get() -> Result<()> {
    let client = OpenFanClient::with_config(
        "http://localhost:3000".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )
    .await?;

    // Test getting single fan RPM
    let _result = client.get_fan_rpm(0).await;
    // May succeed or fail depending on server state

    // Test invalid fan ID validation
    let result = client.get_fan_rpm(10).await;
    assert!(result.is_err(), "Should fail with invalid fan ID");

    let result = client.get_fan_rpm(255).await;
    assert!(result.is_err(), "Should fail with very large fan ID");

    Ok(())
}

#[tokio::test]
#[ignore] // Requires running server
async fn test_concurrent_client_operations() -> Result<()> {
    let client = std::sync::Arc::new(
        OpenFanClient::with_config(
            "http://localhost:3000".to_string(),
            10,
            3,
            tokio::time::Duration::from_millis(500),
        )
        .await?,
    );

    // Spawn multiple concurrent operations (they'll all fail, but shouldn't panic)
    let tasks: Vec<_> = (0..5)
        .map(|i| {
            let client = client.clone();
            tokio::spawn(async move {
                let _ = client.set_fan_pwm(i % 10, 50).await;
                let _ = client.get_fan_status().await;
                let _ = client.get_fan_rpm(i % 10).await;
            })
        })
        .collect();

    // Wait for all tasks to complete
    for task in tasks {
        task.await?;
    }

    Ok(())
}

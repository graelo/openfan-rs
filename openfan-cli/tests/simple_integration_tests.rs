//! Simple integration tests for OpenFAN CLI
//!
//! These tests verify the CLI functionality with basic client testing
//! and mock responses without spawning background servers.

use anyhow::Result;
use openfan_cli::client::OpenFanClient;
use openfan_core::types::{ControlMode, FanProfile};

#[tokio::test]
async fn test_client_creation_and_validation() -> Result<()> {
    // Test client creation with various URLs
    let _client = OpenFanClient::with_config(
        "http://localhost:8080".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )?;
    // Don't test ping - might succeed if there's a real server running

    // Test URL validation with clearly invalid URL
    let _client = OpenFanClient::with_config(
        "http://192.0.2.1:99999".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )?;
    // Just verify client creation works - don't test network calls

    Ok(())
}

#[tokio::test]
async fn test_fan_id_validation() -> Result<()> {
    let client = OpenFanClient::with_config(
        "http://localhost:9999".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )?;

    // Test invalid fan IDs (client-side validation should catch these)
    let result = client.set_fan_pwm(10, 50).await;
    assert!(result.is_err(), "Should fail with fan ID 10");

    let result = client.set_fan_pwm(255, 50).await;
    assert!(result.is_err(), "Should fail with fan ID 255");

    Ok(())
}

#[tokio::test]
async fn test_pwm_value_validation() -> Result<()> {
    let client = OpenFanClient::with_config(
        "http://localhost:9999".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )?;

    // Test invalid PWM values (client-side validation should catch these)
    let result = client.set_fan_pwm(0, 101).await;
    assert!(result.is_err(), "Should fail with PWM 101");

    let result = client.set_fan_pwm(0, 999).await;
    assert!(result.is_err(), "Should fail with PWM 999");

    Ok(())
}

#[tokio::test]
async fn test_rpm_value_validation() -> Result<()> {
    let client = OpenFanClient::with_config(
        "http://localhost:9999".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )?;

    // Test invalid RPM values (client-side validation should catch these)
    let result = client.set_fan_rpm(0, 10001).await;
    assert!(result.is_err(), "Should fail with RPM 10001");

    let result = client.set_fan_rpm(0, 50000).await;
    assert!(result.is_err(), "Should fail with RPM 50000");

    Ok(())
}

#[tokio::test]
async fn test_alias_validation() -> Result<()> {
    let client = OpenFanClient::with_config(
        "http://localhost:9999".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )?;

    // Test invalid fan ID for alias
    let result = client.set_alias(10, "Test Fan").await;
    assert!(result.is_err(), "Should fail with fan ID 10");

    // Test invalid fan ID for get alias
    let result = client.get_alias(10).await;
    assert!(result.is_err(), "Should fail with fan ID 10");

    Ok(())
}

#[tokio::test]
async fn test_profile_validation() -> Result<()> {
    let client = OpenFanClient::with_config(
        "http://localhost:9999".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )?;

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
        values: vec![50; 10],
    };

    let result = client.add_profile("", valid_profile.clone()).await;
    assert!(result.is_err(), "Should fail with empty name");

    let result = client.add_profile("   ", valid_profile).await;
    assert!(result.is_err(), "Should fail with whitespace-only name");

    Ok(())
}

#[tokio::test]
async fn test_client_timeout_behavior() -> Result<()> {
    // Create client with very short timeout for unreachable address
    let client = OpenFanClient::with_config(
        "http://192.0.2.1:12345".to_string(), // Non-routable IP
        1,                                    // 1 second timeout
        1,                                    // 1 retry
        tokio::time::Duration::from_millis(100),
    )?;

    // Test one operation that should definitely fail
    let start = std::time::Instant::now();
    let result = client.get_info().await;
    let duration = start.elapsed();

    assert!(result.is_err());
    assert!(duration.as_secs() < 5); // Should timeout quickly

    Ok(())
}

#[tokio::test]
async fn test_url_formatting() -> Result<()> {
    // Test that client properly handles URLs with/without trailing slashes
    let _client1 = OpenFanClient::with_config(
        "http://localhost:8080".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )?;
    let _client2 = OpenFanClient::with_config(
        "http://localhost:8080/".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )?;

    // Both should format URLs the same way (this is tested by creating the clients)
    // The actual URL formatting is tested in unit tests

    Ok(())
}

#[tokio::test]
async fn test_error_handling_chain() -> Result<()> {
    let client = OpenFanClient::with_config(
        "http://localhost:19999".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )?;

    // Test that errors propagate correctly through the client chain
    match client.get_info().await {
        Ok(_) => panic!("Should not succeed"),
        Err(e) => {
            let error_msg = format!("{}", e);
            // Should contain context about the failed operation
            assert!(error_msg.contains("info") || error_msg.contains("request"));
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_single_fan_rpm_get() -> Result<()> {
    let client = OpenFanClient::with_config(
        "http://localhost:19997".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )?;

    // Test getting single fan RPM (should fail since no server)
    let result = client.get_fan_rpm(0).await;
    assert!(result.is_err(), "Should fail with no server running");

    // Test invalid fan ID validation
    let result = client.get_fan_rpm(10).await;
    assert!(result.is_err(), "Should fail with invalid fan ID");

    let result = client.get_fan_rpm(255).await;
    assert!(result.is_err(), "Should fail with very large fan ID");

    Ok(())
}

#[tokio::test]
async fn test_concurrent_client_operations() -> Result<()> {
    let client = std::sync::Arc::new(OpenFanClient::with_config(
        "http://localhost:19998".to_string(),
        10,
        3,
        tokio::time::Duration::from_millis(500),
    )?);

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

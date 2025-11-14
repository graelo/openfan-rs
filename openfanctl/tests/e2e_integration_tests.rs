//! End-to-End Integration Tests for OpenFAN CLI and Server
//!
//! These tests spawn an actual server process and run CLI commands against it,
//! testing the complete integration from CLI commands through the REST API
//! to the server responses.

use anyhow::Result;
use serde_json::Value;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout};

/// Test configuration
const SERVER_STARTUP_TIMEOUT: Duration = Duration::from_millis(2000);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const SERVER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// E2E Test harness that manages server lifecycle
pub struct E2ETestHarness {
    server_process: Arc<Mutex<Option<Child>>>,
    server_port: u16,
    server_url: String,
    temp_dir: TempDir,
}

impl Default for E2ETestHarness {
    /// Create a new test harness with a unique port
    fn default() -> Self {
        // Use a high port number to avoid conflicts, include thread/process ID for uniqueness
        let thread_id = std::thread::current().id();
        let thread_hash = format!("{:?}", thread_id)
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>();
        let port_offset = thread_hash.parse::<u16>().unwrap_or(0) % 1000;
        let server_port = 18000 + port_offset;
        let server_url = format!("http://127.0.0.1:{}", server_port);
        let temp_dir = tempfile::tempdir().unwrap();

        Self {
            server_process: Arc::new(Mutex::new(None)),
            server_port,
            server_url,
            temp_dir,
        }
    }
}

impl E2ETestHarness {
    /// Start the server in mock mode
    pub async fn start_server(&self) -> Result<()> {
        let mut process_guard = self.server_process.lock().await;

        println!("Starting server on port {}", self.server_port);

        // Create a temporary config file for testing
        let config_content = r#"
server:
  port: 8080
  bind: "127.0.0.1"

hardware:
  device_path: "/dev/ttyUSB0"
  baud_rate: 115200
  timeout_ms: 2000

fans:
  count: 10

profiles: {}
"#;
        let config_path = self
            .temp_dir
            .as_ref()
            .join(format!("test_config_{}.yaml", self.server_port));
        std::fs::write(&config_path, config_content)?;

        // Start the server process in mock mode
        let child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "openfand",
                "--bin",
                "openfand",
                "--",
                "--mock",
                "--config",
                &config_path.to_string_lossy(),
                "--port",
                &self.server_port.to_string(),
                "--bind",
                "127.0.0.1",
            ])
            .current_dir(".")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to start server process");

        // Assign child to process guard immediately
        *process_guard = Some(child);

        // Assign child to process guard immediately
        // Ensure child is only moved once

        // Wait for server to be ready
        let start_time = std::time::Instant::now();
        let mut last_error = String::new();

        while start_time.elapsed() < SERVER_STARTUP_TIMEOUT {
            let poll_start = std::time::Instant::now();
            if let Ok(response) = self.check_server_health().await {
                if response.status().is_success() {
                    // Minimal sleep after health check (reduce from 500ms to 20ms)
                    sleep(Duration::from_millis(20)).await;
                    return Ok(());
                }
                last_error = format!("HTTP status: {}", response.status());
            } else {
                // Check if child process is still running
                match process_guard.as_mut().unwrap().try_wait() {
                    Ok(Some(status)) => {
                        anyhow::bail!("Server process exited early with status {}", status);
                    }
                    Ok(None) => {
                        // Process is still running
                        last_error = "Server process running but not responding".to_string();
                    }
                    Err(e) => {
                        last_error = format!("Error checking process status: {}", e);
                    }
                }
            }
            let _poll_elapsed = poll_start.elapsed();
            sleep(Duration::from_millis(20)).await;
        }

        // Kill the child process if startup failed
        if let Some(ref mut child) = process_guard.as_mut() {
            let _ = child.kill();
        }

        anyhow::bail!(
            "Server failed to start within {} seconds. Last error: {}",
            SERVER_STARTUP_TIMEOUT.as_secs(),
            last_error
        );
    }

    /// Check if server is responding
    async fn check_server_health(&self) -> Result<reqwest::Response> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(1000))
            .build()?;

        client
            .get(format!("{}/api/v0/info", self.server_url))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Health check failed: {}", e))
    }

    /// Stop the server
    pub async fn stop_server(&self) -> Result<()> {
        let mut process_guard = self.server_process.lock().await;
        if let Some(mut child) = process_guard.take() {
            // Try graceful shutdown first
            let _ = child.kill();

            // Wait for process to exit
            match timeout(SERVER_SHUTDOWN_TIMEOUT, async {
                loop {
                    match child.try_wait() {
                        Ok(Some(_)) => break,
                        Ok(None) => sleep(Duration::from_millis(100)).await,
                        Err(_) => break,
                    }
                }
            })
            .await
            {
                Ok(_) => println!("Server stopped gracefully"),
                Err(_) => {
                    println!("Server shutdown timeout, forcing kill");
                    let _ = child.kill();
                }
            }
        }

        Ok(())
    }

    /// Run a CLI command and return the output
    pub async fn run_cli_command(&self, args: &[&str]) -> Result<std::process::Output> {
        let mut cmd_args = vec![
            "run",
            "-p",
            "openfanctl",
            "--bin",
            "openfanctl",
            "--",
            "--server",
            &self.server_url,
            "--no-config",
        ];
        cmd_args.extend(args);

        let output = timeout(COMMAND_TIMEOUT, async {
            Command::new("cargo")
                .args(&cmd_args)
                .current_dir(".")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
        })
        .await??;

        Ok(output)
    }

    /// Run a CLI command and expect success, returning stdout
    pub async fn run_cli_success(&self, args: &[&str]) -> Result<String> {
        let output = self.run_cli_command(args).await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            anyhow::bail!(
                "CLI command failed with status {}: stderr: {}, stdout: {}",
                output.status,
                stderr,
                stdout
            );
        }

        Ok(String::from_utf8(output.stdout)?)
    }

    /// Run a CLI command and expect failure
    pub async fn run_cli_expect_failure(&self, args: &[&str]) -> Result<String> {
        let output = self.run_cli_command(args).await?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            anyhow::bail!("Expected CLI command to fail, but it succeeded: {}", stdout);
        }

        Ok(String::from_utf8(output.stderr)?)
    }
}

impl Drop for E2ETestHarness {
    fn drop(&mut self) {
        // Ensure server process is killed on drop
        if let Ok(mut guard) = self.server_process.try_lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
            }
        }
        // The temp_dir will be automatically cleaned up when it goes out of scope
    }
}

#[tokio::test]
async fn test_e2e_server_startup_and_info() -> Result<()> {
    let harness = E2ETestHarness::default();

    // Start server
    harness.start_server().await?;

    // Test info command
    let output = harness.run_cli_success(&["info"]).await?;
    assert!(
        output.contains("OpenFAN") || output.contains("software"),
        "Output should contain OpenFAN or software info: {}",
        output
    );

    // Test JSON output
    let json_output = harness
        .run_cli_success(&["--format", "json", "info"])
        .await?;
    let _: Value = serde_json::from_str(&json_output)
        .map_err(|e| anyhow::anyhow!("Failed to parse JSON output: {}", e))?;

    harness.stop_server().await?;
    Ok(())
}

#[tokio::test]
async fn test_e2e_fan_status_and_control() -> Result<()> {
    let harness = E2ETestHarness::default();
    harness.start_server().await?;

    // Test fan status
    let status_output = harness.run_cli_success(&["status"]).await?;
    assert!(
        status_output.contains("Fan")
            || status_output.contains("ID")
            || status_output.contains("PWM")
            || status_output.contains("RPM"),
        "Status should show fan information: {}",
        status_output
    );

    // Test setting fan PWM
    let _pwm_output = harness
        .run_cli_success(&["fan", "set", "0", "--pwm", "75"])
        .await?;

    // Test setting fan RPM
    let _rpm_output = harness
        .run_cli_success(&["fan", "set", "1", "--rpm", "2000"])
        .await?;

    // Test getting single fan RPM
    let _fan_rpm = harness.run_cli_success(&["fan", "rpm", "0"]).await?;

    // Test getting single fan PWM
    let _fan_pwm = harness.run_cli_success(&["fan", "pwm", "1"]).await?;

    harness.stop_server().await?;
    Ok(())
}

#[tokio::test]
async fn test_e2e_profile_operations() -> Result<()> {
    let harness = E2ETestHarness::default();
    harness.start_server().await?;

    // Test listing profiles (should be empty initially)
    let list_output = harness.run_cli_success(&["profile", "list"]).await?;
    println!("Initial profiles: {}", list_output);

    // Test adding a profile
    let _add_output = harness
        .run_cli_success(&[
            "profile",
            "add",
            "test_profile",
            "pwm",
            "10,20,30,40,50,60,70,80,90,100",
        ])
        .await?;

    // Test listing profiles again (should show our new profile)
    let list_output2 = harness.run_cli_success(&["profile", "list"]).await?;
    assert!(
        list_output2.contains("test_profile"),
        "Should contain the added profile: {}",
        list_output2
    );

    // Test applying the profile
    let _apply_output = harness
        .run_cli_success(&["profile", "apply", "test_profile"])
        .await?;

    // Test removing the profile
    let _remove_output = harness
        .run_cli_success(&["profile", "remove", "test_profile"])
        .await?;

    // Test listing profiles again (should not contain removed profile)
    let list_output3 = harness.run_cli_success(&["profile", "list"]).await?;
    assert!(
        !list_output3.contains("test_profile"),
        "Profile should be removed: {}",
        list_output3
    );

    harness.stop_server().await?;
    Ok(())
}

#[tokio::test]
async fn test_e2e_alias_operations() -> Result<()> {
    let harness = E2ETestHarness::default();
    harness.start_server().await?;

    // Test listing aliases (should have defaults)
    let list_output = harness.run_cli_success(&["alias", "list"]).await?;
    println!("Initial aliases: {}", list_output);

    // Test setting an alias
    let _set_output = harness
        .run_cli_success(&["alias", "set", "0", "CPU_Fan"])
        .await?;

    // Test getting the alias
    let get_output = harness.run_cli_success(&["alias", "get", "0"]).await?;
    assert!(
        get_output.contains("CPU_Fan"),
        "Should contain the set alias: {}",
        get_output
    );

    // Test listing aliases again
    let list_output2 = harness.run_cli_success(&["alias", "list"]).await?;
    assert!(
        list_output2.contains("CPU_Fan"),
        "Should contain the new alias: {}",
        list_output2
    );

    harness.stop_server().await?;
    Ok(())
}

#[tokio::test]
async fn test_e2e_error_handling() -> Result<()> {
    let harness = E2ETestHarness::default();
    harness.start_server().await?;

    // Test invalid fan ID
    let error_output = harness
        .run_cli_expect_failure(&["fan", "set", "10", "--pwm", "50"])
        .await?;
    assert!(
        error_output.to_lowercase().contains("fan") || error_output.contains("Invalid"),
        "Should show fan ID error: {}",
        error_output
    );

    // Test invalid PWM value
    let error_output2 = harness
        .run_cli_expect_failure(&["fan", "set", "0", "--pwm", "150"])
        .await?;
    assert!(
        error_output2.to_lowercase().contains("pwm")
            || error_output2.contains("range")
            || error_output2.contains("value"),
        "Should show PWM range error: {}",
        error_output2
    );

    // Test invalid RPM value
    let error_output3 = harness
        .run_cli_expect_failure(&["fan", "set", "0", "--rpm", "50000"])
        .await?;
    assert!(
        error_output3.to_lowercase().contains("rpm")
            || error_output3.contains("range")
            || error_output3.contains("value"),
        "Should show RPM range error: {}",
        error_output3
    );

    // Test nonexistent profile
    let error_output4 = harness
        .run_cli_expect_failure(&["profile", "apply", "nonexistent"])
        .await?;
    assert!(
        error_output4.to_lowercase().contains("profile")
            || error_output4.contains("not found")
            || error_output4.contains("exist"),
        "Should show profile not found error: {}",
        error_output4
    );

    harness.stop_server().await?;
    Ok(())
}

#[tokio::test]
async fn test_e2e_json_output_format() -> Result<()> {
    let harness = E2ETestHarness::default();
    harness.start_server().await?;

    // Test JSON info output
    let info_json = harness
        .run_cli_success(&["--format", "json", "info"])
        .await?;
    let info_value: Value = serde_json::from_str(&info_json)?;
    assert!(
        info_value.get("software").is_some() || info_value.as_object().is_some(),
        "JSON should contain software info or be an object: {}",
        info_json
    );

    // Test JSON status output
    let status_json = harness
        .run_cli_success(&["--format", "json", "status"])
        .await?;
    let status_value: Value = serde_json::from_str(&status_json)?;
    assert!(
        status_value.is_array() || status_value.is_object(),
        "JSON status should be structured: {}",
        status_json
    );

    // Test JSON alias list output
    let alias_json = harness
        .run_cli_success(&["--format", "json", "alias", "list"])
        .await?;
    let alias_value: Value = serde_json::from_str(&alias_json)?;
    assert!(
        alias_value.is_object() || alias_value.is_array(),
        "JSON aliases should be structured: {}",
        alias_json
    );

    harness.stop_server().await?;
    Ok(())
}

#[tokio::test]
async fn test_e2e_health_check() -> Result<()> {
    let harness = E2ETestHarness::default();
    harness.start_server().await?;

    // Test health command
    let health_output = harness.run_cli_success(&["health"]).await?;
    assert!(
        health_output.to_lowercase().contains("ok")
            || health_output.to_lowercase().contains("healthy")
            || health_output.to_lowercase().contains("success")
            || health_output.contains("✓")
            || health_output.contains("connected"),
        "Health check should indicate OK status: {}",
        health_output
    );

    harness.stop_server().await?;
    Ok(())
}

#[tokio::test]
async fn test_e2e_server_without_mock_fails_gracefully() -> Result<()> {
    // This test verifies that the server exits cleanly when hardware is not available
    // and --mock flag is not provided

    let server_port = 18500 + (std::process::id() % 100) as u16;

    // Try to start server without --mock flag
    let output = timeout(Duration::from_secs(15), async {
        Command::new("cargo")
            .args([
                "run",
                "-p",
                "openfand",
                "--bin",
                "openfand",
                "--",
                "--port",
                &server_port.to_string(),
                "--bind",
                "127.0.0.1",
            ])
            .output()
    })
    .await??;

    // Server should exit with non-zero status
    assert!(
        !output.status.success(),
        "Server should fail without hardware"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined_output = format!("{} {}", stderr, stdout);

    assert!(
        combined_output.contains("Hardware connection failed")
            || combined_output.contains("--mock")
            || combined_output.contains("cannot start without hardware"),
        "Error message should mention hardware failure or mock flag: {}",
        combined_output
    );

    Ok(())
}

#[tokio::test]
async fn test_e2e_cli_server_connection_failure() -> Result<()> {
    // Test CLI behavior when server is not running
    let harness = E2ETestHarness::default();
    // Note: we don't start the server for this test

    let error_output = harness.run_cli_expect_failure(&["info"]).await?;
    assert!(
        error_output.to_lowercase().contains("connection")
            || error_output.contains("refused")
            || error_output.to_lowercase().contains("failed")
            || error_output.contains("unreachable")
            || error_output.contains("Cannot connect"),
        "Should show connection error: {}",
        error_output
    );

    Ok(())
}

#[tokio::test]
async fn test_e2e_profile_with_different_modes() -> Result<()> {
    let harness = E2ETestHarness::default();
    harness.start_server().await?;

    // Test PWM profile
    let _pwm_add = harness
        .run_cli_success(&[
            "profile",
            "add",
            "pwm_profile",
            "pwm",
            "10,15,20,25,30,35,40,45,50,55",
        ])
        .await?;

    // Test RPM profile
    let _rpm_add = harness
        .run_cli_success(&[
            "profile",
            "add",
            "rpm_profile",
            "rpm",
            "1000,1200,1400,1600,1800,2000,2200,2400,2600,2800",
        ])
        .await?;

    // List profiles to verify both were added
    let list_output = harness.run_cli_success(&["profile", "list"]).await?;
    assert!(
        list_output.contains("pwm_profile"),
        "Should contain PWM profile: {}",
        list_output
    );
    assert!(
        list_output.contains("rpm_profile"),
        "Should contain RPM profile: {}",
        list_output
    );

    // Apply both profiles
    let _apply_pwm = harness
        .run_cli_success(&["profile", "apply", "pwm_profile"])
        .await?;
    let _apply_rpm = harness
        .run_cli_success(&["profile", "apply", "rpm_profile"])
        .await?;

    // Clean up
    let _remove_pwm = harness
        .run_cli_success(&["profile", "remove", "pwm_profile"])
        .await?;
    let _remove_rpm = harness
        .run_cli_success(&["profile", "remove", "rpm_profile"])
        .await?;

    harness.stop_server().await?;
    Ok(())
}

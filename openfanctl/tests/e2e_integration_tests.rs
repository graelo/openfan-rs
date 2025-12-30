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
const SERVER_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
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
        // Use a high port number to avoid conflicts
        // Combine process ID, thread ID, and timestamp for better uniqueness
        let process_id = std::process::id();
        let thread_id = std::thread::current().id();
        let thread_hash = format!("{:?}", thread_id)
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>();
        let thread_num = thread_hash.parse::<u32>().unwrap_or(0);

        // Use timestamp nanoseconds for additional uniqueness
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();

        // Combine to create a unique port in range 20000-29999
        let port_offset = ((process_id + thread_num + nanos) % 10000) as u16;
        let server_port = 20000 + port_offset;
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

        // Create a temporary config file for testing (TOML format)
        let data_dir = self
            .temp_dir
            .as_ref()
            .join(format!("data_{}", self.server_port));
        std::fs::create_dir_all(&data_dir)?;

        let config_content = format!(
            r#"data_dir = "{}"

[server]
hostname = "127.0.0.1"
port = {}
communication_timeout = 1

[hardware]
hostname = "127.0.0.1"
port = 3000
communication_timeout = 1
"#,
            data_dir.display(),
            self.server_port
        );
        let config_path = self
            .temp_dir
            .as_ref()
            .join(format!("test_config_{}.toml", self.server_port));
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
        let mut poll_interval = Duration::from_millis(100);

        while start_time.elapsed() < SERVER_STARTUP_TIMEOUT {
            if let Ok(response) = self.check_server_health().await {
                if response.status().is_success() {
                    // Give server a moment to fully stabilize
                    sleep(Duration::from_millis(100)).await;
                    println!(
                        "Server started successfully on port {} after {:?}",
                        self.server_port,
                        start_time.elapsed()
                    );
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

            // Use exponential backoff for polling: start at 100ms, max out at 500ms
            sleep(poll_interval).await;
            poll_interval = std::cmp::min(poll_interval * 2, Duration::from_millis(500));
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
            .timeout(Duration::from_secs(3))
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

    // Test deleting an alias
    let delete_output = harness
        .run_cli_success(&["alias", "delete", "0"])
        .await?;
    assert!(
        delete_output.contains("Deleted") || delete_output.contains("reverted"),
        "Should confirm deletion: {}",
        delete_output
    );

    // Verify alias is reverted to default
    let get_after_delete = harness.run_cli_success(&["alias", "get", "0"]).await?;
    assert!(
        get_after_delete.contains("Fan #1") || !get_after_delete.contains("CPU_Fan"),
        "Should show default alias after delete: {}",
        get_after_delete
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

#[tokio::test]
async fn test_e2e_thermal_curve_operations() -> Result<()> {
    let harness = E2ETestHarness::default();
    harness.start_server().await?;

    // Test listing curves (should have defaults: Balanced, Silent, Aggressive)
    let list_output = harness.run_cli_success(&["curve", "list"]).await?;
    println!("Initial curves: {}", list_output);
    assert!(
        list_output.contains("Balanced") || list_output.contains("Silent"),
        "Should contain default curves: {}",
        list_output
    );

    // Test getting a specific curve
    let get_output = harness.run_cli_success(&["curve", "get", "Balanced"]).await?;
    assert!(
        get_output.contains("Balanced") && get_output.contains("Points"),
        "Should show curve details: {}",
        get_output
    );

    // Test adding a new curve
    let _add_output = harness
        .run_cli_success(&[
            "curve",
            "add",
            "TestCurve",
            "--points",
            "30:20,50:50,70:80,85:100",
            "--description",
            "Test curve for E2E",
        ])
        .await?;

    // Test listing curves again (should show our new curve)
    let list_output2 = harness.run_cli_success(&["curve", "list"]).await?;
    assert!(
        list_output2.contains("TestCurve"),
        "Should contain the added curve: {}",
        list_output2
    );

    // Test interpolating with the curve
    let interp_output = harness
        .run_cli_success(&["curve", "interpolate", "TestCurve", "--temp", "40.0"])
        .await?;
    assert!(
        interp_output.contains("40") && interp_output.contains("PWM"),
        "Should show interpolation result: {}",
        interp_output
    );

    // Test JSON output for interpolation
    let interp_json = harness
        .run_cli_success(&["--format", "json", "curve", "interpolate", "TestCurve", "--temp", "60.0"])
        .await?;
    let interp_value: Value = serde_json::from_str(&interp_json)?;
    assert!(
        interp_value.get("temperature").is_some() && interp_value.get("pwm").is_some(),
        "JSON should contain temperature and pwm: {}",
        interp_json
    );

    // Test updating the curve
    let _update_output = harness
        .run_cli_success(&[
            "curve",
            "update",
            "TestCurve",
            "--points",
            "25:15,45:45,65:75,80:100",
        ])
        .await?;

    // Test getting the updated curve
    let get_updated = harness.run_cli_success(&["curve", "get", "TestCurve"]).await?;
    assert!(
        get_updated.contains("25") || get_updated.contains("15"),
        "Should show updated curve points: {}",
        get_updated
    );

    // Test deleting the curve
    let delete_output = harness
        .run_cli_success(&["curve", "delete", "TestCurve"])
        .await?;
    assert!(
        delete_output.contains("Deleted") || delete_output.contains("TestCurve"),
        "Should confirm deletion: {}",
        delete_output
    );

    // Verify curve is deleted
    let list_output3 = harness.run_cli_success(&["curve", "list"]).await?;
    assert!(
        !list_output3.contains("TestCurve"),
        "Curve should be removed: {}",
        list_output3
    );

    harness.stop_server().await?;
    Ok(())
}

#[tokio::test]
async fn test_e2e_thermal_curve_errors() -> Result<()> {
    let harness = E2ETestHarness::default();
    harness.start_server().await?;

    // Test getting non-existent curve
    let error_output = harness
        .run_cli_expect_failure(&["curve", "get", "NonExistentCurve"])
        .await?;
    assert!(
        error_output.contains("not exist") || error_output.contains("not found"),
        "Should show curve not found error: {}",
        error_output
    );

    // Test deleting non-existent curve
    let error_output2 = harness
        .run_cli_expect_failure(&["curve", "delete", "NonExistentCurve"])
        .await?;
    assert!(
        error_output2.contains("not exist") || error_output2.contains("not found"),
        "Should show curve not found error: {}",
        error_output2
    );

    // Test interpolating on non-existent curve
    let error_output3 = harness
        .run_cli_expect_failure(&["curve", "interpolate", "NonExistentCurve", "--temp", "50.0"])
        .await?;
    assert!(
        error_output3.contains("not exist") || error_output3.contains("not found"),
        "Should show curve not found error: {}",
        error_output3
    );

    harness.stop_server().await?;
    Ok(())
}

#[tokio::test]
async fn test_e2e_zone_operations() -> Result<()> {
    let harness = E2ETestHarness::default();
    harness.start_server().await?;

    // Test listing zones (should be empty initially)
    let list_output = harness.run_cli_success(&["zone", "list"]).await?;
    println!("Initial zones: {}", list_output);
    assert!(
        list_output.contains("No zones") || !list_output.contains("intake"),
        "Should have no zones initially: {}",
        list_output
    );

    // Test adding a zone
    let add_output = harness
        .run_cli_success(&[
            "zone",
            "add",
            "intake",
            "--ports",
            "0,1,2",
            "--description",
            "Front intake fans",
        ])
        .await?;
    assert!(
        add_output.contains("Added") || add_output.contains("intake"),
        "Should confirm zone creation: {}",
        add_output
    );

    // Test listing zones again (should show our new zone)
    let list_output2 = harness.run_cli_success(&["zone", "list"]).await?;
    assert!(
        list_output2.contains("intake"),
        "Should contain the added zone: {}",
        list_output2
    );

    // Test getting a specific zone
    let get_output = harness.run_cli_success(&["zone", "get", "intake"]).await?;
    assert!(
        get_output.contains("intake") && (get_output.contains("0") || get_output.contains("Ports")),
        "Should show zone details: {}",
        get_output
    );

    // Test JSON output for zone get
    let get_json = harness
        .run_cli_success(&["--format", "json", "zone", "get", "intake"])
        .await?;
    let zone_value: Value = serde_json::from_str(&get_json)?;
    assert!(
        zone_value.get("zone").is_some(),
        "JSON should contain zone: {}",
        get_json
    );

    // Test adding another zone
    let _add_output2 = harness
        .run_cli_success(&[
            "zone",
            "add",
            "exhaust",
            "--ports",
            "3,4",
            "--description",
            "Rear exhaust fans",
        ])
        .await?;

    // Test updating a zone
    let update_output = harness
        .run_cli_success(&[
            "zone",
            "update",
            "intake",
            "--ports",
            "0,1,2,5",
            "--description",
            "Updated intake zone",
        ])
        .await?;
    assert!(
        update_output.contains("Updated") || update_output.contains("intake"),
        "Should confirm zone update: {}",
        update_output
    );

    // Test getting the updated zone
    let get_updated = harness.run_cli_success(&["zone", "get", "intake"]).await?;
    assert!(
        get_updated.contains("5") || get_updated.contains("Updated"),
        "Should show updated zone: {}",
        get_updated
    );

    // Test applying PWM to a zone
    let apply_pwm_output = harness
        .run_cli_success(&["zone", "apply", "intake", "--pwm", "75"])
        .await?;
    assert!(
        apply_pwm_output.contains("Applied") || apply_pwm_output.contains("75"),
        "Should confirm PWM applied: {}",
        apply_pwm_output
    );

    // Test applying RPM to a zone
    let apply_rpm_output = harness
        .run_cli_success(&["zone", "apply", "exhaust", "--rpm", "1500"])
        .await?;
    assert!(
        apply_rpm_output.contains("Applied") || apply_rpm_output.contains("1500"),
        "Should confirm RPM applied: {}",
        apply_rpm_output
    );

    // Test deleting a zone
    let delete_output = harness
        .run_cli_success(&["zone", "delete", "exhaust"])
        .await?;
    assert!(
        delete_output.contains("Deleted") || delete_output.contains("exhaust"),
        "Should confirm deletion: {}",
        delete_output
    );

    // Verify zone is deleted
    let list_output3 = harness.run_cli_success(&["zone", "list"]).await?;
    assert!(
        !list_output3.contains("exhaust"),
        "Zone should be removed: {}",
        list_output3
    );
    assert!(
        list_output3.contains("intake"),
        "Other zone should still exist: {}",
        list_output3
    );

    // Clean up remaining zone
    let _cleanup = harness
        .run_cli_success(&["zone", "delete", "intake"])
        .await?;

    harness.stop_server().await?;
    Ok(())
}

#[tokio::test]
async fn test_e2e_zone_errors() -> Result<()> {
    let harness = E2ETestHarness::default();
    harness.start_server().await?;

    // Test getting non-existent zone
    let error_output = harness
        .run_cli_expect_failure(&["zone", "get", "NonExistentZone"])
        .await?;
    assert!(
        error_output.contains("not exist") || error_output.contains("not found"),
        "Should show zone not found error: {}",
        error_output
    );

    // Test deleting non-existent zone
    let error_output2 = harness
        .run_cli_expect_failure(&["zone", "delete", "NonExistentZone"])
        .await?;
    assert!(
        error_output2.contains("not exist") || error_output2.contains("not found"),
        "Should show zone not found error: {}",
        error_output2
    );

    // Test applying to non-existent zone
    let error_output3 = harness
        .run_cli_expect_failure(&["zone", "apply", "NonExistentZone", "--pwm", "50"])
        .await?;
    assert!(
        error_output3.contains("not exist") || error_output3.contains("not found"),
        "Should show zone not found error: {}",
        error_output3
    );

    // Test updating non-existent zone
    let error_output4 = harness
        .run_cli_expect_failure(&["zone", "update", "NonExistentZone", "--ports", "0,1"])
        .await?;
    assert!(
        error_output4.contains("not exist") || error_output4.contains("not found"),
        "Should show zone not found error: {}",
        error_output4
    );

    harness.stop_server().await?;
    Ok(())
}

#[tokio::test]
async fn test_e2e_cfm_operations() -> Result<()> {
    let harness = E2ETestHarness::default();
    harness.start_server().await?;

    // Test listing CFM mappings (should be empty initially)
    let list_output = harness.run_cli_success(&["cfm", "list"]).await?;
    println!("Initial CFM mappings: {}", list_output);
    assert!(
        list_output.contains("No CFM") || list_output.contains("mappings") || list_output.contains("{}"),
        "Should show empty or no mappings: {}",
        list_output
    );

    // Test setting a CFM mapping
    let set_output = harness
        .run_cli_success(&["cfm", "set", "0", "--cfm-at-100", "45.0"])
        .await?;
    assert!(
        set_output.contains("Set") || set_output.contains("45") || set_output.contains("✓"),
        "Should confirm CFM set: {}",
        set_output
    );

    // Test getting the CFM mapping
    let get_output = harness.run_cli_success(&["cfm", "get", "0"]).await?;
    assert!(
        get_output.contains("45") || get_output.contains("CFM"),
        "Should show CFM value: {}",
        get_output
    );

    // Test JSON output for CFM get
    let get_json = harness
        .run_cli_success(&["--format", "json", "cfm", "get", "0"])
        .await?;
    let cfm_value: Value = serde_json::from_str(&get_json)?;
    assert!(
        cfm_value.get("port").is_some() && cfm_value.get("cfm_at_100").is_some(),
        "JSON should contain port and cfm_at_100: {}",
        get_json
    );

    // Test setting another CFM mapping
    let _set_output2 = harness
        .run_cli_success(&["cfm", "set", "1", "--cfm-at-100", "60.5"])
        .await?;

    // Test listing CFM mappings again (should show our mappings)
    let list_output2 = harness.run_cli_success(&["cfm", "list"]).await?;
    assert!(
        list_output2.contains("45") || list_output2.contains("0"),
        "Should contain first mapping: {}",
        list_output2
    );

    // Test JSON output for CFM list
    let list_json = harness
        .run_cli_success(&["--format", "json", "cfm", "list"])
        .await?;
    let list_value: Value = serde_json::from_str(&list_json)?;
    assert!(
        list_value.get("mappings").is_some(),
        "JSON should contain mappings: {}",
        list_json
    );

    // Test updating a CFM mapping (set again with different value)
    let _update_output = harness
        .run_cli_success(&["cfm", "set", "0", "--cfm-at-100", "50.0"])
        .await?;

    // Verify the update
    let get_updated = harness.run_cli_success(&["cfm", "get", "0"]).await?;
    assert!(
        get_updated.contains("50"),
        "Should show updated CFM value: {}",
        get_updated
    );

    // Test deleting a CFM mapping
    let delete_output = harness.run_cli_success(&["cfm", "delete", "1"]).await?;
    assert!(
        delete_output.contains("Deleted") || delete_output.contains("Removed") || delete_output.contains("✓"),
        "Should confirm deletion: {}",
        delete_output
    );

    // Verify mapping is deleted
    let list_output3 = harness.run_cli_success(&["cfm", "list"]).await?;
    assert!(
        !list_output3.contains("60.5"),
        "Deleted mapping should be removed: {}",
        list_output3
    );

    // Clean up remaining mapping
    let _cleanup = harness.run_cli_success(&["cfm", "delete", "0"]).await?;

    harness.stop_server().await?;
    Ok(())
}

#[tokio::test]
async fn test_e2e_cfm_errors() -> Result<()> {
    let harness = E2ETestHarness::default();
    harness.start_server().await?;

    // Test getting non-existent CFM mapping
    let error_output = harness
        .run_cli_expect_failure(&["cfm", "get", "5"])
        .await?;
    assert!(
        error_output.contains("not") || error_output.contains("No") || error_output.contains("mapping"),
        "Should show no mapping error: {}",
        error_output
    );

    // Test invalid port ID (out of range)
    let error_output2 = harness
        .run_cli_expect_failure(&["cfm", "set", "99", "--cfm-at-100", "45.0"])
        .await?;
    assert!(
        error_output2.to_lowercase().contains("port")
            || error_output2.contains("Invalid")
            || error_output2.contains("range"),
        "Should show invalid port error: {}",
        error_output2
    );

    // Test invalid CFM value (zero)
    let error_output3 = harness
        .run_cli_expect_failure(&["cfm", "set", "0", "--cfm-at-100", "0.0"])
        .await?;
    assert!(
        error_output3.to_lowercase().contains("cfm")
            || error_output3.contains("positive")
            || error_output3.contains("value"),
        "Should show invalid CFM error: {}",
        error_output3
    );

    // Test invalid CFM value (negative)
    let error_output4 = harness
        .run_cli_expect_failure(&["cfm", "set", "0", "--cfm-at-100", "-10.0"])
        .await?;
    assert!(
        error_output4.to_lowercase().contains("cfm")
            || error_output4.contains("positive")
            || error_output4.contains("value"),
        "Should show invalid CFM error for negative: {}",
        error_output4
    );

    // Test invalid CFM value (exceeds maximum)
    let error_output5 = harness
        .run_cli_expect_failure(&["cfm", "set", "0", "--cfm-at-100", "1000.0"])
        .await?;
    assert!(
        error_output5.to_lowercase().contains("cfm")
            || error_output5.contains("500")
            || error_output5.contains("max"),
        "Should show CFM exceeds max error: {}",
        error_output5
    );

    // Test deleting non-existent CFM mapping
    let error_output6 = harness
        .run_cli_expect_failure(&["cfm", "delete", "7"])
        .await?;
    assert!(
        error_output6.contains("not") || error_output6.contains("No") || error_output6.contains("mapping"),
        "Should show no mapping to delete error: {}",
        error_output6
    );

    harness.stop_server().await?;
    Ok(())
}

#[tokio::test]
async fn test_e2e_status_with_cfm() -> Result<()> {
    let harness = E2ETestHarness::default();
    harness.start_server().await?;

    // Get status without CFM mappings (should not show CFM column)
    let status_no_cfm = harness.run_cli_success(&["status"]).await?;
    println!("Status without CFM: {}", status_no_cfm);

    // Add CFM mappings
    let _set1 = harness
        .run_cli_success(&["cfm", "set", "0", "--cfm-at-100", "45.0"])
        .await?;
    let _set2 = harness
        .run_cli_success(&["cfm", "set", "1", "--cfm-at-100", "60.0"])
        .await?;

    // Get status with CFM mappings (should show CFM column)
    let status_with_cfm = harness.run_cli_success(&["status"]).await?;
    println!("Status with CFM: {}", status_with_cfm);
    assert!(
        status_with_cfm.contains("CFM"),
        "Status should show CFM column when mappings exist: {}",
        status_with_cfm
    );

    // Test JSON status output includes CFM
    let status_json = harness
        .run_cli_success(&["--format", "json", "status"])
        .await?;
    let status_value: Value = serde_json::from_str(&status_json)?;
    assert!(
        status_value.get("cfm").is_some() || status_value.get("rpms").is_some(),
        "JSON status should contain cfm or rpms: {}",
        status_json
    );

    // Clean up
    let _del1 = harness.run_cli_success(&["cfm", "delete", "0"]).await?;
    let _del2 = harness.run_cli_success(&["cfm", "delete", "1"]).await?;

    harness.stop_server().await?;
    Ok(())
}

//! Command execution handlers

use anyhow::Result;
use openfan_core::parse_points;
use openfan_core::types::{ControlMode, FanProfile};
use openfan_core::ZoneFan;

use crate::client::OpenFanClient;
use crate::config::CliConfig;
use crate::format::format_success;

use super::commands::*;

/// Default controller ID used when controller is not specified in port format.
const DEFAULT_CONTROLLER_ID: &str = "default";

/// Error message when neither --pwm nor --rpm is specified.
const ERR_PWM_OR_RPM_REQUIRED: &str = "Must specify either --pwm or --rpm";

/// Parse zone port specifications into ZoneFan entries.
///
/// Supports two formats:
/// - Simple: "0,1,2" (uses "default" controller)
/// - Controller-qualified: "main:0,main:1,gpu:0"
/// - Mixed: "0,1,gpu:2" (plain numbers use "default" controller)
fn parse_zone_ports(ports: &str) -> Result<Vec<ZoneFan>> {
    let mut fans = Vec::new();

    for part in ports.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        if let Some((controller, fan_id_str)) = part.split_once(':') {
            // Controller-qualified format: "main:0"
            let fan_id: u8 = fan_id_str
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid fan ID '{}' in '{}'", fan_id_str, part))?;
            fans.push(ZoneFan::new(controller, fan_id));
        } else {
            // Simple format: just a number, uses default controller
            let fan_id: u8 = part
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid port specification '{}'", part))?;
            fans.push(ZoneFan::new(DEFAULT_CONTROLLER_ID, fan_id));
        }
    }

    if fans.is_empty() {
        return Err(anyhow::anyhow!("No valid port specifications provided"));
    }

    Ok(fans)
}

/// Handle info command
pub async fn handle_info(client: &OpenFanClient, format: &OutputFormat) -> Result<()> {
    let info = client.get_info().await?;

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&info)?);
        }
        OutputFormat::Table => {
            let formatted = crate::format::format_info(&info, &format.into())?;
            println!("{}", formatted);
        }
    }

    Ok(())
}

/// Handle status command
pub async fn handle_status(client: &OpenFanClient, format: &OutputFormat) -> Result<()> {
    let status = client.get_fan_status().await?;

    // Try to fetch CFM mappings (optional, don't fail if unavailable)
    let cfm_mappings = client.get_cfm_mappings().await.ok();

    let formatted =
        crate::format::format_fan_status_with_cfm(&status, cfm_mappings.as_ref(), &format.into())?;
    println!("{}", formatted);

    Ok(())
}

/// Handle health command
pub async fn handle_health(client: &OpenFanClient, format: &OutputFormat) -> Result<()> {
    let health = client.health_check().await?;

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&health)?);
        }
        OutputFormat::Table => {
            println!("Server Health Check:");
            println!("{:<20} Value", "Status");
            println!("{}", "-".repeat(40));

            for (key, value) in &health {
                let value_str = match value {
                    serde_json::Value::Bool(b) => {
                        if *b {
                            "✓".to_string()
                        } else {
                            "✗".to_string()
                        }
                    }
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    _ => value.to_string(),
                };
                println!("{:<20} {}", key, value_str);
            }
        }
    }

    Ok(())
}

/// Handle controllers list command
pub async fn handle_controllers_list(client: &OpenFanClient, format: &OutputFormat) -> Result<()> {
    let response = client.list_controllers().await?;

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
        OutputFormat::Table => {
            println!("Controllers ({}):", response.count);
            println!(
                "{:<12} {:<20} {:<6} {:<10} {:<10} Description",
                "ID", "Board", "Fans", "Mock", "Connected"
            );
            println!("{}", "-".repeat(80));

            for ctrl in &response.controllers {
                println!(
                    "{:<12} {:<20} {:<6} {:<10} {:<10} {}",
                    ctrl.id,
                    ctrl.board_name,
                    ctrl.fan_count,
                    if ctrl.mock_mode { "yes" } else { "no" },
                    if ctrl.connected { "yes" } else { "no" },
                    ctrl.description.as_deref().unwrap_or("")
                );
            }
        }
    }

    Ok(())
}

/// Handle controller subcommands
pub async fn handle_controller(
    client: &OpenFanClient,
    command: ControllerCommands,
    format: &OutputFormat,
) -> Result<()> {
    match command {
        ControllerCommands::Info { id } => {
            let info = client.get_controller_info(&id).await?;

            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&info)?);
                }
                OutputFormat::Table => {
                    println!("Controller: {}", info.id);
                    println!("  Board:      {}", info.board_name);
                    println!("  Fans:       {}", info.fan_count);
                    println!(
                        "  Mock mode:  {}",
                        if info.mock_mode { "yes" } else { "no" }
                    );
                    println!(
                        "  Connected:  {}",
                        if info.connected { "yes" } else { "no" }
                    );
                    if let Some(desc) = &info.description {
                        println!("  Description: {}", desc);
                    }
                }
            }
        }
        ControllerCommands::Reconnect { id } => {
            let message = client.reconnect_controller(&id).await?;
            println!("{}", format_success(&message));
        }
    }

    Ok(())
}

/// Handle fan commands
pub async fn handle_fan(
    client: &OpenFanClient,
    command: FanCommands,
    format: &OutputFormat,
) -> Result<()> {
    match command {
        FanCommands::Set { fan_id, pwm, rpm } => match (pwm, rpm) {
            (Some(pwm), None) => {
                client.set_fan_pwm(fan_id, pwm).await?;
                println!(
                    "{}",
                    format_success(&format!("Set fan {} to {}% PWM", fan_id, pwm))
                );
            }
            (None, Some(rpm)) => {
                client.set_fan_rpm(fan_id, rpm).await?;
                println!(
                    "{}",
                    format_success(&format!("Set fan {} to {} RPM", fan_id, rpm))
                );
            }
            _ => {
                return Err(anyhow::anyhow!(ERR_PWM_OR_RPM_REQUIRED));
            }
        },
        FanCommands::Rpm { fan_id } => {
            let rpm_response = client.get_fan_rpm(fan_id).await?;

            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&rpm_response)?);
                }
                OutputFormat::Table => {
                    println!("Fan {} RPM: {}", fan_id, rpm_response.rpm);
                }
            }
        }
        FanCommands::Pwm { fan_id } => {
            let status = client.get_fan_status_by_id(fan_id).await?;
            let pwm = status.pwms.get(&fan_id).unwrap_or(&0);

            match format {
                OutputFormat::Json => {
                    let response = serde_json::json!({
                        "fan_id": fan_id,
                        "pwm": pwm
                    });
                    println!("{}", serde_json::to_string_pretty(&response)?);
                }
                OutputFormat::Table => {
                    println!("Fan {} PWM: {}%", fan_id, pwm);
                }
            }
        }
    }

    Ok(())
}

/// Handle profile commands
pub async fn handle_profile(
    client: &OpenFanClient,
    command: ProfileCommands,
    format: &OutputFormat,
) -> Result<()> {
    match command {
        ProfileCommands::List => {
            let profiles = client.get_profiles().await?;

            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&profiles)?);
                }
                OutputFormat::Table => {
                    let formatted = crate::format::format_profiles(&profiles, &format.into())?;
                    println!("{}", formatted);
                }
            }
        }
        ProfileCommands::Apply { name } => {
            client.apply_profile(&name).await?;
            println!("{}", format_success(&format!("Applied profile: {}", name)));
        }
        ProfileCommands::Add { name, mode, values } => {
            let values_vec: Result<Vec<u32>, _> =
                values.split(',').map(|s| s.trim().parse::<u32>()).collect();

            let values_vec = values_vec?;

            let control_mode = match mode {
                ProfileMode::Pwm => ControlMode::Pwm,
                ProfileMode::Rpm => ControlMode::Rpm,
            };

            let profile = FanProfile {
                control_mode,
                values: values_vec,
            };

            client.add_profile(&name, profile).await?;
            println!("{}", format_success(&format!("Added profile: {}", name)));
        }
        ProfileCommands::Remove { name } => {
            client.remove_profile(&name).await?;
            println!("{}", format_success(&format!("Removed profile: {}", name)));
        }
    }

    Ok(())
}

/// Handle alias commands
pub async fn handle_alias(
    client: &OpenFanClient,
    command: AliasCommands,
    format: &OutputFormat,
) -> Result<()> {
    match command {
        AliasCommands::List => {
            let aliases = client.get_aliases().await?;

            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&aliases)?);
                }
                OutputFormat::Table => {
                    let formatted = crate::format::format_aliases(&aliases, &format.into())?;
                    println!("{}", formatted);
                }
            }
        }
        AliasCommands::Get { fan_id } => {
            let alias_response = client.get_alias(fan_id).await?;
            let default_alias = format!("Fan #{}", fan_id);
            let alias = alias_response
                .aliases
                .get(&fan_id)
                .unwrap_or(&default_alias);

            match format {
                OutputFormat::Json => {
                    let response = serde_json::json!({
                        "fan_id": fan_id,
                        "alias": alias
                    });
                    println!("{}", serde_json::to_string_pretty(&response)?);
                }
                OutputFormat::Table => {
                    println!("Fan {} alias: {}", fan_id, alias);
                }
            }
        }
        AliasCommands::Set { fan_id, name } => {
            client.set_alias(fan_id, &name).await?;
            println!(
                "{}",
                format_success(&format!("Set alias for fan {} to: {}", fan_id, name))
            );
        }
        AliasCommands::Delete { fan_id } => {
            client.delete_alias(fan_id).await?;
            println!(
                "{}",
                format_success(&format!(
                    "Deleted alias for fan {} (reverted to default)",
                    fan_id
                ))
            );
        }
    }

    Ok(())
}

/// Handle zone commands
pub async fn handle_zone(
    client: &OpenFanClient,
    command: ZoneCommands,
    format: &OutputFormat,
) -> Result<()> {
    match command {
        ZoneCommands::List => {
            let zones = client.get_zones().await?;

            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&zones)?);
                }
                OutputFormat::Table => {
                    if zones.zones.is_empty() {
                        println!("No zones configured.");
                    } else {
                        println!("{:<20} {:<40} Description", "Name", "Fans");
                        println!("{}", "-".repeat(80));
                        for (name, zone) in &zones.zones {
                            let fans_str = zone
                                .fans
                                .iter()
                                .map(|f| format!("{}:{}", f.controller, f.fan_id))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let desc = zone.description.as_deref().unwrap_or("-");
                            println!("{:<20} {:<40} {}", name, fans_str, desc);
                        }
                    }
                }
            }
        }
        ZoneCommands::Get { name } => {
            let response = client.get_zone(&name).await?;
            let zone = &response.zone;

            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&response)?);
                }
                OutputFormat::Table => {
                    let fans_str = zone
                        .fans
                        .iter()
                        .map(|f| format!("{}:{}", f.controller, f.fan_id))
                        .collect::<Vec<_>>()
                        .join(", ");
                    println!("Zone: {}", zone.name);
                    println!("Fans: {}", fans_str);
                    if let Some(desc) = &zone.description {
                        println!("Description: {}", desc);
                    }
                }
            }
        }
        ZoneCommands::Add {
            name,
            ports,
            description,
        } => {
            let fans = parse_zone_ports(&ports)?;
            client.add_zone(&name, fans, description).await?;
            println!("{}", format_success(&format!("Added zone: {}", name)));
        }
        ZoneCommands::Update {
            name,
            ports,
            description,
        } => {
            let fans = parse_zone_ports(&ports)?;
            client.update_zone(&name, fans, description).await?;
            println!("{}", format_success(&format!("Updated zone: {}", name)));
        }
        ZoneCommands::Delete { name } => {
            client.delete_zone(&name).await?;
            println!("{}", format_success(&format!("Deleted zone: {}", name)));
        }
        ZoneCommands::Apply { name, pwm, rpm } => match (pwm, rpm) {
            (Some(pwm), None) => {
                client.apply_zone(&name, "pwm", pwm).await?;
                println!(
                    "{}",
                    format_success(&format!("Applied {}% PWM to zone '{}'", pwm, name))
                );
            }
            (None, Some(rpm)) => {
                client.apply_zone(&name, "rpm", rpm).await?;
                println!(
                    "{}",
                    format_success(&format!("Applied {} RPM to zone '{}'", rpm, name))
                );
            }
            _ => {
                return Err(anyhow::anyhow!(ERR_PWM_OR_RPM_REQUIRED));
            }
        },
    }

    Ok(())
}

/// Handle curve commands
pub async fn handle_curve(
    client: &OpenFanClient,
    command: CurveCommands,
    format: &OutputFormat,
) -> Result<()> {
    match command {
        CurveCommands::List => {
            let curves = client.get_curves().await?;

            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&curves)?);
                }
                OutputFormat::Table => {
                    if curves.curves.is_empty() {
                        println!("No thermal curves configured.");
                    } else {
                        println!("{:<20} {:<40} Description", "Name", "Points");
                        println!("{}", "-".repeat(80));
                        for (name, curve) in &curves.curves {
                            let points_str = curve
                                .points
                                .iter()
                                .map(|p| format!("{}:{}", p.temp_c, p.pwm))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let desc = curve.description.as_deref().unwrap_or("-");
                            println!("{:<20} {:<40} {}", name, points_str, desc);
                        }
                    }
                }
            }
        }
        CurveCommands::Get { name } => {
            let response = client.get_curve(&name).await?;
            let curve = &response.curve;

            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&response)?);
                }
                OutputFormat::Table => {
                    let points_str = curve
                        .points
                        .iter()
                        .map(|p| format!("{}°C:{}%", p.temp_c, p.pwm))
                        .collect::<Vec<_>>()
                        .join(", ");
                    println!("Curve: {}", curve.name);
                    println!("Points: {}", points_str);
                    if let Some(desc) = &curve.description {
                        println!("Description: {}", desc);
                    }
                }
            }
        }
        CurveCommands::Add {
            name,
            points,
            description,
        } => {
            let curve_points = parse_points(&points)
                .map_err(|e| anyhow::anyhow!("Invalid points format: {}", e))?;

            client.add_curve(&name, curve_points, description).await?;
            println!("{}", format_success(&format!("Added curve: {}", name)));
        }
        CurveCommands::Update {
            name,
            points,
            description,
        } => {
            let curve_points = parse_points(&points)
                .map_err(|e| anyhow::anyhow!("Invalid points format: {}", e))?;

            client
                .update_curve(&name, curve_points, description)
                .await?;
            println!("{}", format_success(&format!("Updated curve: {}", name)));
        }
        CurveCommands::Delete { name } => {
            client.delete_curve(&name).await?;
            println!("{}", format_success(&format!("Deleted curve: {}", name)));
        }
        CurveCommands::Interpolate { name, temp } => {
            let response = client.interpolate_curve(&name, temp).await?;

            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&response)?);
                }
                OutputFormat::Table => {
                    println!(
                        "Curve '{}' at {}°C = {}% PWM",
                        name, response.temperature, response.pwm
                    );
                }
            }
        }
    }

    Ok(())
}

/// Handle config commands
pub async fn handle_config(
    command: ConfigCommands,
    current_config: &CliConfig,
    format: &OutputFormat,
) -> Result<()> {
    match command {
        ConfigCommands::Show => match format {
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(current_config)?);
            }
            OutputFormat::Table => {
                println!("CLI Configuration:");
                println!("{:<20} Value", "Setting");
                println!("{}", "-".repeat(40));
                println!("{:<20} {}", "Server URL", current_config.server_url);
                println!("{:<20} {}", "Output Format", current_config.output_format);
                println!("{:<20} {}", "Verbose", current_config.verbose);
                println!("{:<20} {}s", "Timeout", current_config.timeout);
            }
        },
        ConfigCommands::Set { key, value } => {
            let mut config = current_config.clone();
            let value_clone = value.clone();
            match key.as_str() {
                "server_url" => config.server_url = value,
                "output_format" => {
                    if ["table", "json"].contains(&value.as_str()) {
                        config.output_format = value;
                    } else {
                        return Err(anyhow::anyhow!(
                            "Invalid output format. Must be 'table' or 'json'"
                        ));
                    }
                }
                "verbose" => {
                    config.verbose = value.to_lowercase() == "true" || value == "1";
                }
                "timeout" => {
                    config.timeout = value
                        .parse()
                        .map_err(|_| anyhow::anyhow!("Invalid timeout value. Must be a number"))?;
                }
                _ => return Err(anyhow::anyhow!("Unknown config key: {}", key)),
            }

            config.save()?;
            println!(
                "{}",
                format_success(&format!("Set {} = {}", key, value_clone))
            );
        }
        ConfigCommands::Reset => {
            let default_config = CliConfig::default();
            default_config.save()?;
            println!("{}", format_success("Configuration reset to defaults"));
        }
    }

    Ok(())
}

/// Handle CFM mapping commands
pub async fn handle_cfm(
    client: &OpenFanClient,
    command: CfmCommands,
    format: &OutputFormat,
) -> Result<()> {
    match command {
        CfmCommands::List => {
            let cfm_response = client.get_cfm_mappings().await?;

            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&cfm_response)?);
                }
                OutputFormat::Table => {
                    if cfm_response.mappings.is_empty() {
                        println!("No CFM mappings configured.");
                    } else {
                        println!("{:<10} CFM@100%", "Port");
                        println!("{}", "-".repeat(25));
                        let mut entries: Vec<_> = cfm_response.mappings.iter().collect();
                        entries.sort_by_key(|(port, _)| *port);
                        for (port, cfm) in entries {
                            println!("{:<10} {:.1}", port, cfm);
                        }
                    }
                }
            }
        }
        CfmCommands::Get { port } => {
            let cfm_response = client.get_cfm(port).await?;

            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&cfm_response)?);
                }
                OutputFormat::Table => {
                    println!("Port {} CFM@100%: {:.1}", port, cfm_response.cfm_at_100);
                }
            }
        }
        CfmCommands::Set { port, cfm_at_100 } => {
            client.set_cfm(port, cfm_at_100).await?;
            println!(
                "{}",
                format_success(&format!(
                    "Set CFM@100% for port {} to {:.1}",
                    port, cfm_at_100
                ))
            );
        }
        CfmCommands::Delete { port } => {
            client.delete_cfm(port).await?;
            println!(
                "{}",
                format_success(&format!("Deleted CFM mapping for port {}", port))
            );
        }
    }

    Ok(())
}

/// Generate shell completion script
pub fn generate_completion(shell: clap_complete::Shell) {
    use clap::CommandFactory;
    use clap_complete::generate;
    use std::io;

    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();
    generate(shell, &mut cmd, bin_name, &mut io::stdout());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::OpenFanClient;
    use crate::test_utils::MockServer;
    use std::time::Duration;

    /// Create a test client connected to a mock server
    async fn create_test_client() -> (MockServer, OpenFanClient) {
        let mock = MockServer::new();
        let (mock, url) = mock.start().await.unwrap();
        let client = OpenFanClient::with_config(url, 10, 3, Duration::from_millis(500))
            .await
            .unwrap();
        (mock, client)
    }

    // ==================== handle_info tests ====================

    #[tokio::test]
    async fn test_handle_info_json() {
        let (_mock, client) = create_test_client().await;
        let result = handle_info(&client, &OutputFormat::Json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_info_table() {
        let (_mock, client) = create_test_client().await;
        let result = handle_info(&client, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    // ==================== handle_status tests ====================

    #[tokio::test]
    async fn test_handle_status_json() {
        let (_mock, client) = create_test_client().await;
        let result = handle_status(&client, &OutputFormat::Json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_status_table() {
        let (_mock, client) = create_test_client().await;
        let result = handle_status(&client, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    // ==================== handle_fan tests ====================

    #[tokio::test]
    async fn test_handle_fan_set_pwm() {
        let (_mock, client) = create_test_client().await;
        let command = FanCommands::Set {
            fan_id: 0,
            pwm: Some(75),
            rpm: None,
        };
        let result = handle_fan(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_fan_set_rpm() {
        let (_mock, client) = create_test_client().await;
        let command = FanCommands::Set {
            fan_id: 0,
            pwm: None,
            rpm: Some(1500),
        };
        let result = handle_fan(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_fan_set_neither_pwm_nor_rpm() {
        let (_mock, client) = create_test_client().await;
        let command = FanCommands::Set {
            fan_id: 0,
            pwm: None,
            rpm: None,
        };
        let result = handle_fan(&client, command, &OutputFormat::Table).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Must specify either --pwm or --rpm"));
    }

    #[tokio::test]
    async fn test_handle_fan_get_rpm_json() {
        let (_mock, client) = create_test_client().await;
        let command = FanCommands::Rpm { fan_id: 0 };
        let result = handle_fan(&client, command, &OutputFormat::Json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_fan_get_rpm_table() {
        let (_mock, client) = create_test_client().await;
        let command = FanCommands::Rpm { fan_id: 0 };
        let result = handle_fan(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_fan_get_pwm_json() {
        let (_mock, client) = create_test_client().await;
        let command = FanCommands::Pwm { fan_id: 0 };
        let result = handle_fan(&client, command, &OutputFormat::Json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_fan_get_pwm_table() {
        let (_mock, client) = create_test_client().await;
        let command = FanCommands::Pwm { fan_id: 0 };
        let result = handle_fan(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    // ==================== handle_profile tests ====================

    #[tokio::test]
    async fn test_handle_profile_list_json() {
        let (_mock, client) = create_test_client().await;
        let command = ProfileCommands::List;
        let result = handle_profile(&client, command, &OutputFormat::Json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_profile_list_table() {
        let (_mock, client) = create_test_client().await;
        let command = ProfileCommands::List;
        let result = handle_profile(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_profile_apply() {
        let (_mock, client) = create_test_client().await;
        let command = ProfileCommands::Apply {
            name: "50% PWM".to_string(),
        };
        let result = handle_profile(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_profile_add_pwm() {
        let (_mock, client) = create_test_client().await;
        let command = ProfileCommands::Add {
            name: "test_profile".to_string(),
            mode: ProfileMode::Pwm,
            values: "50,50,50,50,50,50,50,50,50,50".to_string(),
        };
        let result = handle_profile(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_profile_add_rpm() {
        let (_mock, client) = create_test_client().await;
        let command = ProfileCommands::Add {
            name: "test_rpm_profile".to_string(),
            mode: ProfileMode::Rpm,
            values: "1000,1000,1000,1000,1000,1000,1000,1000,1000,1000".to_string(),
        };
        let result = handle_profile(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_profile_add_invalid_values() {
        let (_mock, client) = create_test_client().await;
        let command = ProfileCommands::Add {
            name: "bad_profile".to_string(),
            mode: ProfileMode::Pwm,
            values: "not,valid,numbers".to_string(),
        };
        let result = handle_profile(&client, command, &OutputFormat::Table).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_profile_remove() {
        let (_mock, client) = create_test_client().await;
        let command = ProfileCommands::Remove {
            name: "50% PWM".to_string(),
        };
        let result = handle_profile(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    // ==================== handle_alias tests ====================

    #[tokio::test]
    async fn test_handle_alias_list_json() {
        let (_mock, client) = create_test_client().await;
        let command = AliasCommands::List;
        let result = handle_alias(&client, command, &OutputFormat::Json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_alias_list_table() {
        let (_mock, client) = create_test_client().await;
        let command = AliasCommands::List;
        let result = handle_alias(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_alias_get_json() {
        let (_mock, client) = create_test_client().await;
        let command = AliasCommands::Get { fan_id: 0 };
        let result = handle_alias(&client, command, &OutputFormat::Json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_alias_get_table() {
        let (_mock, client) = create_test_client().await;
        let command = AliasCommands::Get { fan_id: 0 };
        let result = handle_alias(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_alias_set() {
        let (_mock, client) = create_test_client().await;
        let command = AliasCommands::Set {
            fan_id: 0,
            name: "CPU Fan".to_string(),
        };
        let result = handle_alias(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    // ==================== handle_zone tests ====================

    #[tokio::test]
    async fn test_handle_zone_list_json() {
        let (_mock, client) = create_test_client().await;
        let command = ZoneCommands::List;
        let result = handle_zone(&client, command, &OutputFormat::Json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_zone_list_table() {
        let (_mock, client) = create_test_client().await;
        let command = ZoneCommands::List;
        let result = handle_zone(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_zone_get_json() {
        let (_mock, client) = create_test_client().await;
        let command = ZoneCommands::Get {
            name: "cpu".to_string(),
        };
        let result = handle_zone(&client, command, &OutputFormat::Json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_zone_get_table() {
        let (_mock, client) = create_test_client().await;
        let command = ZoneCommands::Get {
            name: "cpu".to_string(),
        };
        let result = handle_zone(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_zone_add() {
        let (_mock, client) = create_test_client().await;
        let command = ZoneCommands::Add {
            name: "new_zone".to_string(),
            ports: "4,5,6".to_string(),
            description: Some("New zone for testing".to_string()),
        };
        let result = handle_zone(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_zone_update() {
        let (_mock, client) = create_test_client().await;
        let command = ZoneCommands::Update {
            name: "cpu".to_string(),
            ports: "0,1,2".to_string(),
            description: Some("Updated CPU zone".to_string()),
        };
        let result = handle_zone(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_zone_delete() {
        let (_mock, client) = create_test_client().await;
        let command = ZoneCommands::Delete {
            name: "cpu".to_string(),
        };
        let result = handle_zone(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_zone_apply_pwm() {
        let (_mock, client) = create_test_client().await;
        let command = ZoneCommands::Apply {
            name: "cpu".to_string(),
            pwm: Some(75),
            rpm: None,
        };
        let result = handle_zone(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_zone_apply_rpm() {
        let (_mock, client) = create_test_client().await;
        let command = ZoneCommands::Apply {
            name: "cpu".to_string(),
            pwm: None,
            rpm: Some(1500),
        };
        let result = handle_zone(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_zone_apply_neither() {
        let (_mock, client) = create_test_client().await;
        let command = ZoneCommands::Apply {
            name: "cpu".to_string(),
            pwm: None,
            rpm: None,
        };
        let result = handle_zone(&client, command, &OutputFormat::Table).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Must specify either --pwm or --rpm"));
    }

    // ==================== handle_curve tests ====================

    #[tokio::test]
    async fn test_handle_curve_list_json() {
        let (_mock, client) = create_test_client().await;
        let command = CurveCommands::List;
        let result = handle_curve(&client, command, &OutputFormat::Json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_curve_list_table() {
        let (_mock, client) = create_test_client().await;
        let command = CurveCommands::List;
        let result = handle_curve(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_curve_get_json() {
        let (_mock, client) = create_test_client().await;
        let command = CurveCommands::Get {
            name: "default".to_string(),
        };
        let result = handle_curve(&client, command, &OutputFormat::Json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_curve_get_table() {
        let (_mock, client) = create_test_client().await;
        let command = CurveCommands::Get {
            name: "default".to_string(),
        };
        let result = handle_curve(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_curve_add() {
        let (_mock, client) = create_test_client().await;
        let command = CurveCommands::Add {
            name: "aggressive".to_string(),
            points: "20:30,40:50,60:80,80:100".to_string(),
            description: Some("Aggressive cooling curve".to_string()),
        };
        let result = handle_curve(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_curve_add_invalid_points() {
        let (_mock, client) = create_test_client().await;
        let command = CurveCommands::Add {
            name: "bad_curve".to_string(),
            points: "invalid:points".to_string(),
            description: None,
        };
        let result = handle_curve(&client, command, &OutputFormat::Table).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_curve_update() {
        let (_mock, client) = create_test_client().await;
        let command = CurveCommands::Update {
            name: "default".to_string(),
            points: "25:20,50:50,75:80,90:100".to_string(),
            description: Some("Updated default curve".to_string()),
        };
        let result = handle_curve(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_curve_delete() {
        let (_mock, client) = create_test_client().await;
        let command = CurveCommands::Delete {
            name: "default".to_string(),
        };
        let result = handle_curve(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_curve_interpolate_json() {
        let (_mock, client) = create_test_client().await;
        let command = CurveCommands::Interpolate {
            name: "default".to_string(),
            temp: 45.0,
        };
        let result = handle_curve(&client, command, &OutputFormat::Json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_curve_interpolate_table() {
        let (_mock, client) = create_test_client().await;
        let command = CurveCommands::Interpolate {
            name: "default".to_string(),
            temp: 60.0,
        };
        let result = handle_curve(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    // ==================== handle_cfm tests ====================

    #[tokio::test]
    async fn test_handle_cfm_list_json() {
        let (_mock, client) = create_test_client().await;
        let command = CfmCommands::List;
        let result = handle_cfm(&client, command, &OutputFormat::Json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_cfm_list_table() {
        let (_mock, client) = create_test_client().await;
        let command = CfmCommands::List;
        let result = handle_cfm(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_cfm_get_json() {
        let (_mock, client) = create_test_client().await;
        let command = CfmCommands::Get { port: 0 };
        let result = handle_cfm(&client, command, &OutputFormat::Json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_cfm_get_table() {
        let (_mock, client) = create_test_client().await;
        let command = CfmCommands::Get { port: 0 };
        let result = handle_cfm(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_cfm_set() {
        let (_mock, client) = create_test_client().await;
        let command = CfmCommands::Set {
            port: 2,
            cfm_at_100: 55.0,
        };
        let result = handle_cfm(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_cfm_delete() {
        let (_mock, client) = create_test_client().await;
        let command = CfmCommands::Delete { port: 0 };
        let result = handle_cfm(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    // ==================== handle_alias delete test ====================

    #[tokio::test]
    async fn test_handle_alias_delete() {
        let (_mock, client) = create_test_client().await;
        let command = AliasCommands::Delete { fan_id: 0 };
        let result = handle_alias(&client, command, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    // ==================== handle_health tests ====================

    #[tokio::test]
    async fn test_handle_health_json() {
        let (_mock, client) = create_test_client().await;
        let result = handle_health(&client, &OutputFormat::Json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_health_table() {
        let (_mock, client) = create_test_client().await;
        let result = handle_health(&client, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    // ==================== handle_config tests ====================

    #[tokio::test]
    async fn test_handle_config_show_json() {
        let config = CliConfig::default();
        let result = handle_config(ConfigCommands::Show, &config, &OutputFormat::Json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_config_show_table() {
        let config = CliConfig::default();
        let result = handle_config(ConfigCommands::Show, &config, &OutputFormat::Table).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_config_set_invalid_key() {
        let config = CliConfig::default();
        let command = ConfigCommands::Set {
            key: "invalid_key".to_string(),
            value: "some_value".to_string(),
        };
        let result = handle_config(command, &config, &OutputFormat::Table).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown config key"));
    }

    #[tokio::test]
    async fn test_handle_config_set_invalid_output_format() {
        let config = CliConfig::default();
        let command = ConfigCommands::Set {
            key: "output_format".to_string(),
            value: "invalid".to_string(),
        };
        let result = handle_config(command, &config, &OutputFormat::Table).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid output format"));
    }

    #[tokio::test]
    async fn test_handle_config_set_invalid_timeout() {
        let config = CliConfig::default();
        let command = ConfigCommands::Set {
            key: "timeout".to_string(),
            value: "not_a_number".to_string(),
        };
        let result = handle_config(command, &config, &OutputFormat::Table).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid timeout value"));
    }

    // ==================== parse_zone_ports tests ====================

    #[test]
    fn test_parse_zone_ports_simple_format() {
        let result = super::parse_zone_ports("0,1,2").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].controller, "default");
        assert_eq!(result[0].fan_id, 0);
        assert_eq!(result[1].controller, "default");
        assert_eq!(result[1].fan_id, 1);
        assert_eq!(result[2].controller, "default");
        assert_eq!(result[2].fan_id, 2);
    }

    #[test]
    fn test_parse_zone_ports_qualified_format() {
        let result = super::parse_zone_ports("main:0,main:1,gpu:0").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].controller, "main");
        assert_eq!(result[0].fan_id, 0);
        assert_eq!(result[1].controller, "main");
        assert_eq!(result[1].fan_id, 1);
        assert_eq!(result[2].controller, "gpu");
        assert_eq!(result[2].fan_id, 0);
    }

    #[test]
    fn test_parse_zone_ports_mixed_format() {
        let result = super::parse_zone_ports("0,gpu:2,1").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].controller, "default");
        assert_eq!(result[0].fan_id, 0);
        assert_eq!(result[1].controller, "gpu");
        assert_eq!(result[1].fan_id, 2);
        assert_eq!(result[2].controller, "default");
        assert_eq!(result[2].fan_id, 1);
    }

    #[test]
    fn test_parse_zone_ports_with_whitespace() {
        let result = super::parse_zone_ports(" 0 , 1 , main:2 ").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].controller, "default");
        assert_eq!(result[0].fan_id, 0);
        assert_eq!(result[1].controller, "default");
        assert_eq!(result[1].fan_id, 1);
        assert_eq!(result[2].controller, "main");
        assert_eq!(result[2].fan_id, 2);
    }

    #[test]
    fn test_parse_zone_ports_empty_fails() {
        let result = super::parse_zone_ports("");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No valid port specifications"));
    }

    #[test]
    fn test_parse_zone_ports_invalid_number_fails() {
        let result = super::parse_zone_ports("0,abc,2");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid port specification"));
    }

    #[test]
    fn test_parse_zone_ports_invalid_qualified_format_fails() {
        let result = super::parse_zone_ports("main:abc");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid fan ID"));
    }
}

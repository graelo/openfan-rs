//! Command execution handlers

use anyhow::Result;
use openfan_core::types::{ControlMode, FanProfile};

use crate::client::OpenFanClient;
use crate::config::CliConfig;
use crate::format::format_success;

use super::commands::*;

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

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&status)?);
        }
        OutputFormat::Table => {
            let formatted = crate::format::format_fan_status(&status, &format.into())?;
            println!("{}", formatted);
        }
    }

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
                        if *b { "✓".to_string() } else { "✗".to_string() }
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

/// Handle fan commands
pub async fn handle_fan(
    client: &OpenFanClient,
    command: FanCommands,
    format: &OutputFormat,
) -> Result<()> {
    match command {
        FanCommands::Set { fan_id, pwm, rpm } => {
            match (pwm, rpm) {
                (Some(pwm), None) => {
                    client.set_fan_pwm(fan_id, pwm).await?;
                    println!("{}", format_success(&format!("Set fan {} to {}% PWM", fan_id, pwm)));
                }
                (None, Some(rpm)) => {
                    client.set_fan_rpm(fan_id, rpm).await?;
                    println!("{}", format_success(&format!("Set fan {} to {} RPM", fan_id, rpm)));
                }
                _ => {
                    return Err(anyhow::anyhow!("Must specify either --pwm or --rpm"));
                }
            }
        }
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
            let alias = alias_response.aliases.get(&fan_id).unwrap_or(&default_alias);

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
                format_success(&format!("Deleted alias for fan {} (reverted to default)", fan_id))
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
                        println!("{:<20} {:<30} Description", "Name", "Ports");
                        println!("{}", "-".repeat(70));
                        for (name, zone) in &zones.zones {
                            let ports_str = zone
                                .port_ids
                                .iter()
                                .map(|p| p.to_string())
                                .collect::<Vec<_>>()
                                .join(", ");
                            let desc = zone.description.as_deref().unwrap_or("-");
                            println!("{:<20} {:<30} {}", name, ports_str, desc);
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
                    let ports_str = zone
                        .port_ids
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    println!("Zone: {}", zone.name);
                    println!("Ports: {}", ports_str);
                    if let Some(desc) = &zone.description {
                        println!("Description: {}", desc);
                    }
                }
            }
        }
        ZoneCommands::Add { name, ports, description } => {
            let port_ids: Result<Vec<u8>, _> =
                ports.split(',').map(|s| s.trim().parse::<u8>()).collect();
            let port_ids = port_ids?;

            client.add_zone(&name, port_ids, description).await?;
            println!("{}", format_success(&format!("Added zone: {}", name)));
        }
        ZoneCommands::Update { name, ports, description } => {
            let port_ids: Result<Vec<u8>, _> =
                ports.split(',').map(|s| s.trim().parse::<u8>()).collect();
            let port_ids = port_ids?;

            client.update_zone(&name, port_ids, description).await?;
            println!("{}", format_success(&format!("Updated zone: {}", name)));
        }
        ZoneCommands::Delete { name } => {
            client.delete_zone(&name).await?;
            println!("{}", format_success(&format!("Deleted zone: {}", name)));
        }
        ZoneCommands::Apply { name, pwm, rpm } => {
            match (pwm, rpm) {
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
                    return Err(anyhow::anyhow!("Must specify either --pwm or --rpm"));
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
            println!("{}", format_success(&format!("Set {} = {}", key, value_clone)));
        }
        ConfigCommands::Reset => {
            let default_config = CliConfig::default();
            default_config.save()?;
            println!("{}", format_success("Configuration reset to defaults"));
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

//! Output formatting utilities for the CLI
//!
//! Provides table and JSON formatting with colors.

use anyhow::Result;
use colored::*;
use openfan_core::api::{AliasResponse, FanStatusResponse, InfoResponse, ProfileResponse};

use tabled::{settings::Style, Table, Tabled};

/// Output format options
#[derive(Debug, Clone)]
pub enum OutputFormat {
    Table,
    Json,
}

/// Format info response
pub fn format_info(info: &InfoResponse, format: &OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(info)?),
        OutputFormat::Table => {
            let mut output = String::new();
            output.push_str(&"OpenFAN Server Information".bold().to_string());
            output.push('\n');
            output.push_str(&format!("Version: {}", info.version.cyan()));
            output.push('\n');
            output.push_str(&format!(
                "Hardware Connected: {}",
                if info.hardware_connected {
                    "Yes".green()
                } else {
                    "No".red()
                }
            ));
            output.push('\n');
            output.push_str(&format!(
                "Uptime: {} seconds",
                info.uptime.to_string().yellow()
            ));
            output.push('\n');
            output.push_str(&format!("Software: {}", info.software.cyan()));

            if let Some(hardware) = &info.hardware {
                output.push('\n');
                output.push_str(&format!("Hardware: {}", hardware.cyan()));
            }

            if let Some(firmware) = &info.firmware {
                output.push('\n');
                output.push_str(&format!("Firmware: {}", firmware.cyan()));
            }

            Ok(output)
        }
    }
}

/// Format fan status response
pub fn format_fan_status(status: &FanStatusResponse, format: &OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(status)?),
        OutputFormat::Table => {
            #[derive(Tabled)]
            struct FanRow {
                #[tabled(rename = "Fan ID")]
                fan_id: String,
                #[tabled(rename = "RPM")]
                rpm: String,
                #[tabled(rename = "PWM %")]
                pwm: String,
            }

            let mut rows = Vec::new();
            // Collect all fan IDs from both rpms and pwms maps
            let mut fan_ids: Vec<u8> = status
                .rpms
                .keys()
                .chain(status.pwms.keys())
                .copied()
                .collect();
            fan_ids.sort_unstable();
            fan_ids.dedup();

            for fan_id in fan_ids {
                let rpm = status.rpms.get(&fan_id).unwrap_or(&0);
                let pwm = status.pwms.get(&fan_id).unwrap_or(&0);

                rows.push(FanRow {
                    fan_id: format!("{}", fan_id),
                    rpm: if *rpm > 0 {
                        format!("{}", rpm).green().to_string()
                    } else {
                        "0".red().to_string()
                    },
                    pwm: if *pwm > 0 {
                        format!("{}%", pwm).cyan().to_string()
                    } else {
                        "0%".dimmed().to_string()
                    },
                });
            }

            let table = Table::new(rows).with(Style::rounded()).to_string();
            Ok(format!("{}\n{}", "Fan Status:".bold(), table))
        }
    }
}

/// Format profiles response
pub fn format_profiles(profiles: &ProfileResponse, format: &OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(profiles)?),
        OutputFormat::Table => {
            #[derive(Tabled)]
            struct ProfileRow {
                #[tabled(rename = "Profile Name")]
                name: String,
                #[tabled(rename = "Mode")]
                mode: String,
                #[tabled(rename = "Values")]
                values: String,
            }

            let mut rows = Vec::new();
            for (name, profile) in &profiles.profiles {
                rows.push(ProfileRow {
                    name: name.clone().cyan().to_string(),
                    mode: format!("{:?}", profile.control_mode).yellow().to_string(),
                    values: profile
                        .values
                        .iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                });
            }

            let table = Table::new(rows).with(Style::rounded()).to_string();
            Ok(format!("{}\n{}", "Available Profiles:".bold(), table))
        }
    }
}

/// Format aliases response
pub fn format_aliases(aliases: &AliasResponse, format: &OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(aliases)?),
        OutputFormat::Table => {
            #[derive(Tabled)]
            struct AliasRow {
                #[tabled(rename = "Fan ID")]
                fan_id: String,
                #[tabled(rename = "Alias")]
                alias: String,
            }

            let mut rows = Vec::new();
            // Get all fan IDs from the aliases map and sort them
            let mut fan_ids: Vec<&u8> = aliases.aliases.keys().collect();
            fan_ids.sort_unstable();

            for &&fan_id in &fan_ids {
                let alias = aliases
                    .aliases
                    .get(&fan_id)
                    .cloned()
                    .unwrap_or_else(|| format!("Fan #{}", fan_id));

                rows.push(AliasRow {
                    fan_id: format!("{}", fan_id),
                    alias: alias.green().to_string(),
                });
            }

            let table = Table::new(rows).with(Style::rounded()).to_string();
            Ok(format!("{}\n{}", "Fan Aliases:".bold(), table))
        }
    }
}

/// Format success message
pub fn format_success(message: &str) -> String {
    format!("{} {}", "✓".green().bold(), message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfan_core::{
        types::{ControlMode, FanProfile},
        BoardConfig, DefaultBoard,
    };
    use std::collections::HashMap;

    #[test]
    fn test_format_success() {
        let message = format_success("Operation completed");
        assert!(message.contains("✓"));
        assert!(message.contains("Operation completed"));
    }

    #[test]
    fn test_format_info_json() {
        let board_info = openfan_core::BoardType::OpenFanV1.to_board_info();
        let info = InfoResponse {
            version: "1.0.0".to_string(),
            board_info,
            hardware_connected: true,
            uptime: 3600,
            software: "OpenFAN Server v1.0.0".to_string(),
            hardware: Some("Hardware v1.0".to_string()),
            firmware: Some("Firmware v1.0".to_string()),
        };

        let result = format_info(&info, &OutputFormat::Json).unwrap();
        assert!(result.contains("version"));
        assert!(result.contains("1.0.0"));
        assert!(result.contains("hardware_connected"));
        assert!(result.contains("true"));
    }

    #[test]
    fn test_format_fan_status_json() {
        let mut rpms = HashMap::new();
        let mut pwms = HashMap::new();
        rpms.insert(0, 1200);
        rpms.insert(1, 1500);
        pwms.insert(0, 50);
        pwms.insert(1, 75);

        let status = FanStatusResponse { rpms, pwms };
        let result = format_fan_status(&status, &OutputFormat::Json).unwrap();

        assert!(result.contains("rpms"));
        assert!(result.contains("pwms"));
        assert!(result.contains("1200"));
        assert!(result.contains("1500"));
    }

    #[test]
    fn test_format_profiles_json() {
        let mut profiles = HashMap::new();
        profiles.insert(
            "Test Profile".to_string(),
            FanProfile {
                control_mode: ControlMode::Pwm,
                values: vec![50; DefaultBoard::FAN_COUNT],
            },
        );

        let response = ProfileResponse { profiles };
        let result = format_profiles(&response, &OutputFormat::Json).unwrap();

        assert!(result.contains("Test Profile"));
        assert!(result.contains("type"));
    }

    #[test]
    fn test_format_aliases_json() {
        let mut aliases = HashMap::new();
        aliases.insert(0, "CPU Fan".to_string());
        aliases.insert(1, "Case Fan".to_string());

        let response = AliasResponse { aliases };
        let result = format_aliases(&response, &OutputFormat::Json).unwrap();

        assert!(result.contains("CPU Fan"));
        assert!(result.contains("Case Fan"));
    }
}

//! OpenFAN CLI
//!
//! Command-line interface for controlling the OpenFAN server.

mod client;
mod config;
mod format;

#[cfg(test)]
mod test_utils;

use anyhow::Result;
use clap::{Parser, Subcommand};
use client::OpenFanClient;
use config::CliConfig;
use format::format_success;
use openfan_core::types::{ControlMode, FanProfile};

/// OpenFAN Controller CLI
#[derive(Parser, Debug)]
#[command(name = "openfan")]
#[command(version, about = "OpenFAN Controller CLI", long_about = None)]
struct Cli {
    /// Server URL (overrides config file)
    #[arg(short, long)]
    server: Option<String>,

    /// Output format (overrides config file)
    #[arg(short, long, value_enum)]
    format: Option<OutputFormat>,

    /// Enable verbose logging (overrides config file)
    #[arg(short, long)]
    verbose: Option<bool>,

    /// Don't load config file
    #[arg(long)]
    no_config: bool,

    /// Config file path (default: ~/.config/openfan/cli.toml)
    #[arg(long)]
    config: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum OutputFormat {
    /// Pretty table output
    Table,
    /// JSON output
    Json,
}

impl From<&OutputFormat> for format::OutputFormat {
    fn from(format: &OutputFormat) -> Self {
        match format {
            OutputFormat::Table => format::OutputFormat::Table,
            OutputFormat::Json => format::OutputFormat::Json,
        }
    }
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show system information
    Info,

    /// Show fan status
    Status,

    /// Check server connectivity and health
    Health,

    /// Show or manage CLI configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Fan control commands
    Fan {
        #[command(subcommand)]
        command: FanCommands,
    },

    /// Profile management commands
    Profile {
        #[command(subcommand)]
        command: ProfileCommands,
    },

    /// Alias management commands
    Alias {
        #[command(subcommand)]
        command: AliasCommands,
    },

    /// Generate shell completion scripts
    Completion {
        /// Shell to generate completion for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand, Debug)]
enum FanCommands {
    /// Set fan speed
    Set {
        /// Fan ID (0-9)
        fan_id: u8,

        /// PWM percentage (0-100)
        #[arg(long)]
        pwm: Option<u32>,

        /// Target RPM
        #[arg(long)]
        rpm: Option<u32>,
    },

    /// Get fan RPM
    Rpm {
        /// Fan ID (0-9)
        fan_id: u8,
    },

    /// Get fan PWM
    Pwm {
        /// Fan ID (0-9)
        fan_id: u8,
    },
}

#[derive(Subcommand, Debug)]
enum ProfileCommands {
    /// List all profiles
    List,

    /// Apply a profile
    Apply {
        /// Profile name
        name: String,
    },

    /// Add a new profile
    Add {
        /// Profile name
        name: String,

        /// Control mode (pwm or rpm)
        #[arg(value_enum)]
        mode: ProfileMode,

        /// Comma-separated values (10 values)
        values: String,
    },

    /// Remove a profile
    Remove {
        /// Profile name
        name: String,
    },
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum ProfileMode {
    Pwm,
    Rpm,
}

#[derive(Subcommand, Debug)]
enum AliasCommands {
    /// Show all aliases
    List,

    /// Get alias for a fan
    Get {
        /// Fan ID (0-9)
        fan_id: u8,
    },

    /// Set alias for a fan
    Set {
        /// Fan ID (0-9)
        fan_id: u8,

        /// Alias name
        name: String,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCommands {
    /// Show current configuration
    Show,

    /// Set configuration value
    Set {
        /// Configuration key
        key: String,
        /// Configuration value
        value: String,
    },

    /// Reset configuration to defaults
    Reset,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Build configuration using priority chain: defaults → file → env → CLI args
    let mut builder = CliConfig::builder();

    // Load config file (unless --no-config is specified)
    builder = builder.with_config_file(!cli.no_config)?;

    // Apply environment variable overrides
    builder = builder.with_env_overrides();

    // Apply CLI argument overrides (highest priority)
    if let Some(ref server) = cli.server {
        builder = builder.with_server_url(server)?;
    }
    if let Some(ref format) = cli.format {
        let format_str = match format {
            OutputFormat::Table => "table",
            OutputFormat::Json => "json",
        };
        builder = builder.with_output_format(format_str)?;
    }
    if let Some(verbose) = cli.verbose {
        builder = builder.with_verbose(verbose);
    }

    // Build final configuration with validation
    let config = match builder.build() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Configuration error: {}", e);
            if cli.verbose.unwrap_or(false) {
                eprintln!("Error details: {:?}", e);
            }
            std::process::exit(1);
        }
    };

    // Determine final settings from validated config
    let server_url = &config.server_url;
    let output_format = match config.output_format.as_str() {
        "json" => OutputFormat::Json,
        _ => OutputFormat::Table,
    };
    let verbose = config.verbose;

    // Initialize logging if verbose
    if verbose {
        eprintln!("Verbose mode enabled");
        eprintln!("Server URL: {}", server_url);
        eprintln!("Output format: {:?}", output_format);
    }

    // Create HTTP client with config-based timeout
    // This fetches board info from the server during initialization
    if verbose {
        eprintln!("Connecting to server and fetching board info...");
    }

    let client = match OpenFanClient::with_config(
        server_url.clone(),
        config.timeout,
        3,                                     // max_retries
        std::time::Duration::from_millis(500), // retry_delay
    )
    .await
    {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Error: Cannot connect to OpenFAN server at {}", server_url);
            eprintln!("Make sure the server is running and accessible.");
            eprintln!("Connection error: {}", e);
            std::process::exit(1);
        }
    };

    if verbose {
        eprintln!("Successfully connected to server");
        eprintln!(
            "Board: {} ({} fans)",
            client.board_info().name,
            client.board_info().fan_count
        );
    }

    // Execute commands with proper error handling
    let result = match cli.command {
        Commands::Info => handle_info(&client, &output_format).await,
        Commands::Status => handle_status(&client, &output_format).await,
        Commands::Health => handle_health(&client, &output_format).await,
        Commands::Config { command } => {
            handle_config_command(command, &config, &output_format).await
        }
        Commands::Fan { command } => handle_fan_command(&client, command, &output_format).await,
        Commands::Profile { command } => {
            handle_profile_command(&client, command, &output_format).await
        }
        Commands::Alias { command } => handle_alias_command(&client, command, &output_format).await,
        Commands::Completion { shell } => {
            generate_completion(shell);
            Ok(())
        }
    };

    // Handle command execution errors gracefully
    if let Err(e) = result {
        eprintln!("Error: {}", e);
        if verbose {
            eprintln!("Error details: {:?}", e);
        }
        std::process::exit(1);
    }

    Ok(())
}

/// Handle info command
async fn handle_info(client: &OpenFanClient, format: &OutputFormat) -> Result<()> {
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
async fn handle_status(client: &OpenFanClient, format: &OutputFormat) -> Result<()> {
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

/// Handle fan commands
async fn handle_fan_command(
    client: &OpenFanClient,
    command: FanCommands,
    format: &OutputFormat,
) -> Result<()> {
    match command {
        FanCommands::Set { fan_id, pwm, rpm } => {
            // Validation is now handled by client methods using board info
            match (pwm, rpm) {
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
                    return Err(anyhow::anyhow!("Must specify either --pwm or --rpm"));
                }
            }
        }
        FanCommands::Rpm { fan_id } => {
            // Validation is handled by client method
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
            // Validation is handled by client method
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
async fn handle_profile_command(
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
            // Validation of value count is handled by client method using board info

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
async fn handle_alias_command(
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
            // Validation is handled by client method
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
            // Validation is handled by client method
            client.set_alias(fan_id, &name).await?;
            println!(
                "{}",
                format_success(&format!("Set alias for fan {} to: {}", fan_id, name))
            );
        }
    }

    Ok(())
}

/// Generate shell completion script
fn generate_completion(shell: clap_complete::Shell) {
    use clap::CommandFactory;
    use clap_complete::generate;
    use std::io;

    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();
    generate(shell, &mut cmd, bin_name, &mut io::stdout());
}

/// Handle health command
async fn handle_health(client: &OpenFanClient, format: &OutputFormat) -> Result<()> {
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

/// Handle config commands
async fn handle_config_command(
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

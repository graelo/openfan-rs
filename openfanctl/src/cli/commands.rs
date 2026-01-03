//! CLI command and subcommand definitions

use clap::{Parser, Subcommand};

/// OpenFAN Controller CLI
#[derive(Parser, Debug)]
#[command(name = "openfanctl")]
#[command(version, about = "OpenFAN Controller CLI", long_about = None)]
pub struct Cli {
    /// Server URL (overrides config file)
    #[arg(short, long)]
    pub server: Option<String>,

    /// Output format (overrides config file)
    #[arg(short, long, value_enum)]
    pub format: Option<OutputFormat>,

    /// Enable verbose logging (overrides config file)
    #[arg(short, long)]
    pub verbose: Option<bool>,

    /// Don't load config file
    #[arg(long)]
    pub no_config: bool,

    /// Config file path (default: ~/.config/openfan/cli.toml)
    #[arg(long)]
    pub config: Option<String>,

    /// Controller ID for controller-specific commands
    ///
    /// Required for fan, profile, alias, curve, and CFM commands in multi-controller setups.
    /// Zone commands are global and don't require this flag.
    #[arg(short = 'c', long, global = true)]
    pub controller: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum OutputFormat {
    /// Pretty table output
    Table,
    /// JSON output
    Json,
}

impl From<&OutputFormat> for crate::format::OutputFormat {
    fn from(format: &OutputFormat) -> Self {
        match format {
            OutputFormat::Table => crate::format::OutputFormat::Table,
            OutputFormat::Json => crate::format::OutputFormat::Json,
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Show system information
    Info,

    /// Show fan status
    Status,

    /// Check server connectivity and health
    Health,

    /// List all controllers (multi-controller management)
    Controllers,

    /// Controller management commands
    Controller {
        #[command(subcommand)]
        command: ControllerCommands,
    },

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

    /// Zone management commands (global, cross-controller)
    Zone {
        #[command(subcommand)]
        command: ZoneCommands,
    },

    /// Thermal curve management commands
    Curve {
        #[command(subcommand)]
        command: CurveCommands,
    },

    /// CFM mapping management commands
    Cfm {
        #[command(subcommand)]
        command: CfmCommands,
    },

    /// Generate shell completion scripts
    Completion {
        /// Shell to generate completion for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand, Debug)]
pub enum ControllerCommands {
    /// Get info for a specific controller
    Info {
        /// Controller ID
        id: String,
    },

    /// Force reconnection for a controller
    Reconnect {
        /// Controller ID
        id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum FanCommands {
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
pub enum ProfileCommands {
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
pub enum ProfileMode {
    Pwm,
    Rpm,
}

#[derive(Subcommand, Debug)]
pub enum AliasCommands {
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

    /// Delete alias for a fan (reverts to default)
    Delete {
        /// Fan ID (0-9)
        fan_id: u8,
    },
}

#[derive(Subcommand, Debug)]
pub enum ZoneCommands {
    /// List all zones
    List,

    /// Get zone details
    Get {
        /// Zone name
        name: String,
    },

    /// Add a new zone
    Add {
        /// Zone name
        name: String,

        /// Comma-separated port specifications.
        ///
        /// Format: "controller:fan_id" or just "fan_id" (uses default controller).
        /// Examples: "0,1,2" or "main:0,main:1,gpu:0"
        #[arg(short, long)]
        ports: String,

        /// Optional description
        #[arg(short, long)]
        description: Option<String>,
    },

    /// Update an existing zone
    Update {
        /// Zone name
        name: String,

        /// Comma-separated port specifications.
        ///
        /// Format: "controller:fan_id" or just "fan_id" (uses default controller).
        /// Examples: "0,1,2" or "main:0,main:1,gpu:0"
        #[arg(short, long)]
        ports: String,

        /// Optional description
        #[arg(short, long)]
        description: Option<String>,
    },

    /// Delete a zone
    Delete {
        /// Zone name
        name: String,
    },

    /// Apply a value to all fans in a zone
    Apply {
        /// Zone name
        name: String,

        /// PWM percentage (0-100)
        #[arg(long)]
        pwm: Option<u16>,

        /// Target RPM (0-16000)
        #[arg(long)]
        rpm: Option<u16>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
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

#[derive(Subcommand, Debug)]
pub enum CurveCommands {
    /// List all thermal curves
    List,

    /// Get thermal curve details
    Get {
        /// Curve name
        name: String,
    },

    /// Add a new thermal curve
    Add {
        /// Curve name
        name: String,

        /// Curve points as "temp:pwm,temp:pwm,..." (e.g., "30:25,50:50,70:80,85:100")
        #[arg(short, long)]
        points: String,

        /// Optional description
        #[arg(short, long)]
        description: Option<String>,
    },

    /// Update an existing thermal curve
    Update {
        /// Curve name
        name: String,

        /// Curve points as "temp:pwm,temp:pwm,..." (e.g., "30:25,50:50,70:80,85:100")
        #[arg(short, long)]
        points: String,

        /// Optional description
        #[arg(short, long)]
        description: Option<String>,
    },

    /// Delete a thermal curve
    Delete {
        /// Curve name
        name: String,
    },

    /// Interpolate PWM value for a given temperature
    Interpolate {
        /// Curve name
        name: String,

        /// Temperature in Celsius
        #[arg(short, long)]
        temp: f32,
    },
}

#[derive(Subcommand, Debug)]
pub enum CfmCommands {
    /// List all CFM mappings
    List,

    /// Get CFM mapping for a port
    Get {
        /// Port ID (0-9)
        port: u8,
    },

    /// Set CFM@100% value for a port
    Set {
        /// Port ID (0-9)
        port: u8,

        /// CFM value at 100% PWM
        #[arg(long)]
        cfm_at_100: f32,
    },

    /// Delete CFM mapping for a port
    Delete {
        /// Port ID (0-9)
        port: u8,
    },
}

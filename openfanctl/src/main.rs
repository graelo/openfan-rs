//! OpenFAN CLI
//!
//! Command-line interface for controlling the OpenFAN server.

use anyhow::Result;
use clap::Parser;
use openfanctl::cli::{
    generate_completion, handle_alias, handle_cfm, handle_config, handle_controller,
    handle_controllers_list, handle_curve, handle_fan, handle_health, handle_info, handle_profile,
    handle_status, handle_zone, Cli, Commands, OutputFormat,
};
use openfanctl::client::OpenFanClient;
use openfanctl::config::CliConfig;

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
    if verbose {
        eprintln!("Connecting to server and fetching board info...");
    }

    let client = match OpenFanClient::with_config(
        server_url.clone(),
        config.timeout,
        3,
        std::time::Duration::from_millis(500),
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

    // Execute commands
    let result = match cli.command {
        Commands::Info => handle_info(&client, &output_format).await,
        Commands::Status => handle_status(&client, &output_format).await,
        Commands::Health => handle_health(&client, &output_format).await,
        Commands::Controllers => handle_controllers_list(&client, &output_format).await,
        Commands::Controller { command } => {
            handle_controller(&client, command, &output_format).await
        }
        Commands::Config { command } => handle_config(command, &config, &output_format).await,
        Commands::Fan { command } => handle_fan(&client, command, &output_format).await,
        Commands::Profile { command } => handle_profile(&client, command, &output_format).await,
        Commands::Alias { command } => handle_alias(&client, command, &output_format).await,
        Commands::Zone { command } => handle_zone(&client, command, &output_format).await,
        Commands::Curve { command } => handle_curve(&client, command, &output_format).await,
        Commands::Cfm { command } => handle_cfm(&client, command, &output_format).await,
        Commands::Completion { shell } => {
            generate_completion(shell);
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        if verbose {
            eprintln!("Error details: {:?}", e);
        }
        std::process::exit(1);
    }

    Ok(())
}

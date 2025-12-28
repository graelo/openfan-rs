//! OpenFAN Server
//!
//! REST API server for controlling fan hardware via serial communication.

mod api;
mod config;
mod hardware;

use anyhow::Result;
use api::AppState;
use clap::Parser;
use config::ConfigManager;
use hardware::connection;
use openfan_core::BoardType;
use std::path::PathBuf;
use std::str::FromStr;
use tokio::signal;
use tracing::{error, info, warn};

/// OpenFAN API Server
#[derive(Parser, Debug)]
#[command(name = "openfand")]
#[command(version, about = "OpenFAN Controller API Server", long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "config.yaml")]
    config: PathBuf,

    /// Server bind address
    #[arg(short, long, default_value = "127.0.0.1")]
    bind: String,

    /// Server port
    #[arg(short, long)]
    port: Option<u16>,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Enable mock mode (run without hardware for testing/development)
    #[arg(long)]
    mock: bool,

    /// Board type to emulate in mock mode (v1, mini)
    #[arg(long, default_value = "v1", requires = "mock")]
    board: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize tracing
    init_tracing(args.verbose);

    info!("OpenFAN Server starting...");
    info!("Configuration file: {}", args.config.display());

    // Step 1: Detect board type (before loading config)
    let board_type = if args.mock {
        info!("Mock mode enabled - running without hardware");
        BoardType::from_str(&args.board).unwrap_or_else(|e| {
            error!("Invalid board type '{}': {}", args.board, e);
            std::process::exit(1);
        })
    } else {
        info!("Detecting hardware board type...");
        match hardware::detect_board_from_usb() {
            Ok(board) => {
                info!("Detected board: {}", board.name());
                board
            }
            Err(e) => {
                error!(
                    "Failed to detect hardware board: {}. Use --mock flag to run in mock mode.",
                    e
                );
                std::process::exit(1);
            }
        }
    };

    let board_info = board_type.to_board_info();
    info!(
        "Board: {} ({} fans, VID:0x{:04X}, PID:0x{:04X})",
        board_info.name, board_info.fan_count, board_info.usb_vid, board_info.usb_pid
    );

    // Step 2: Load configuration
    let mut config_manager = ConfigManager::new(&args.config);
    config_manager.load().await?;
    info!("Configuration loaded successfully");

    // Step 3: Validate configuration against detected board
    match config_manager.validate_for_board(&board_info) {
        Ok(()) => {
            info!(
                "Configuration validated successfully for {}",
                board_info.name
            );
        }
        Err(errors) => {
            error!("Configuration validation failed!");
            error!(
                "Board detected: {} ({} fans)",
                board_info.name, board_info.fan_count
            );
            error!("Configuration file: {}", args.config.display());
            error!("");
            for err in &errors {
                error!("  - {}", err);
            }
            error!("");
            error!("Please update your configuration file to match your board.");
            error!("You can delete the file to regenerate defaults for your board.");
            std::process::exit(1);
        }
    }

    // Auto-fill missing defaults
    config_manager.fill_defaults_for_board(&board_info).await?;

    // Get server config
    let server_config = config_manager.config().server.clone();
    let port = args.port.unwrap_or(server_config.port);
    let bind_addr = format!("{}:{}", args.bind, port);

    // Step 4: Initialize hardware connection
    let fan_controller = if args.mock {
        None
    } else {
        info!("Initializing hardware connection...");
        match connection::auto_connect(2000, args.verbose).await {
            Ok(mut controller) => {
                info!("Hardware connected successfully");

                // Test the connection
                if let Err(e) = connection::test_connection(&mut controller).await {
                    warn!("Hardware test failed, but continuing: {}", e);
                }

                info!("Hardware layer ready");
                Some(controller)
            }
            Err(e) => {
                error!(
                    "Hardware connection failed: {}. Server cannot start without hardware. Use --mock flag to run in mock mode.",
                    e
                );
                std::process::exit(1);
            }
        }
    };

    // Step 5: Create application state with board info
    let app_state = AppState::new(board_info, config_manager, fan_controller);

    // Set up API router
    let app = api::create_router(app_state);

    // Start server
    info!("Starting server on {}", bind_addr);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;

    info!("OpenFAN API Server listening on {}", bind_addr);
    info!("Phase 3 API layer complete. Server ready!");

    // Run server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Server shutdown complete");
    Ok(())
}

/// Wait for shutdown signal
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, shutting down gracefully...");
        },
        _ = terminate => {
            info!("Received SIGTERM, shutting down gracefully...");
        },
    }
}

/// Initialize tracing subscriber for logging
fn init_tracing(verbose: bool) {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let filter = if verbose {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"))
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

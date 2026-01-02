//! OpenFAN Server
//!
//! REST API server for controlling fan hardware via serial communication.

mod api;
mod config;
mod hardware;
mod shutdown;

use anyhow::Result;
use api::AppState;
use clap::Parser;
use config::RuntimeConfig;
use hardware::{connection, ConnectionManager};
use openfan_core::{default_config_path, BoardType};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tokio::signal;
use tracing::{error, info, warn};

/// OpenFAN API Server
#[derive(Parser, Debug)]
#[command(name = "openfand")]
#[command(version, about = "OpenFAN Controller API Server", long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long)]
    config: Option<PathBuf>,

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

    /// Board type (standard, custom:N where N is fan count 1-16)
    ///
    /// Required with --mock or --device. For auto-detection, omit this flag.
    #[arg(long, default_value = "standard")]
    board: String,

    /// Serial device path for custom boards (e.g., /dev/ttyACM0, /dev/ttyUSB0)
    ///
    /// Use this with --board to connect to custom/DIY hardware.
    /// Bypasses USB VID/PID auto-detection.
    #[arg(long, conflicts_with = "mock")]
    device: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize tracing
    init_tracing(args.verbose);

    info!("OpenFAN Server starting...");

    // Determine config path: CLI flag > env var > default
    let config_path = args.config.unwrap_or_else(|| {
        std::env::var("OPENFAN_SERVER_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| default_config_path())
    });
    info!("Configuration file: {}", config_path.display());

    // Step 1: Determine board type and connection mode
    let (board_type, device_path) = if args.mock {
        // Mock mode - no hardware
        info!("Mock mode enabled - running without hardware");
        let board = BoardType::from_str(&args.board).unwrap_or_else(|e| {
            error!("Invalid board type '{}': {}", args.board, e);
            std::process::exit(1);
        });
        (board, None)
    } else if let Some(ref device) = args.device {
        // Direct device connection - use specified board type
        info!("Using specified device: {}", device);
        let board = BoardType::from_str(&args.board).unwrap_or_else(|e| {
            error!("Invalid board type '{}': {}", args.board, e);
            std::process::exit(1);
        });
        (board, Some(device.clone()))
    } else {
        // Auto-detect via USB VID/PID
        info!("Detecting hardware board type...");
        match hardware::detect_board_from_usb() {
            Ok(board) => {
                info!("Detected board: {}", board.name());
                (board, None)
            }
            Err(e) => {
                error!(
                    "Failed to detect hardware board: {}. Use --mock flag for testing, or --device to specify the serial device.",
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
    let runtime_config = RuntimeConfig::load(&config_path).await?;
    info!("Configuration loaded successfully");
    info!("  Static config: {}", config_path.display());
    info!("  Data directory: {}", runtime_config.data_dir().display());

    // Step 3: Validate configuration against detected board
    if let Err(e) = runtime_config.validate_for_board(&board_info).await {
        error!("Configuration validation failed: {}", e);
        std::process::exit(1);
    }
    info!(
        "Configuration validated successfully for {}",
        board_info.name
    );

    // Auto-fill missing defaults
    runtime_config.fill_defaults_for_board(&board_info).await?;

    // Get server config
    let server_config = &runtime_config.static_config().server;
    let port = args.port.unwrap_or(server_config.port);
    let bind_addr = format!("{}:{}", args.bind, port);

    // Step 4: Initialize hardware connection with reconnection support
    let connection_manager = if args.mock {
        None
    } else {
        info!("Initializing hardware connection...");
        let timeout_ms = server_config.communication_timeout * 1000;

        // Use specified device path or auto-detect
        let connect_result = if let Some(ref device) = device_path {
            connection::connect_to_device(device, timeout_ms, args.verbose).await
        } else {
            connection::auto_connect(timeout_ms, args.verbose).await
        };

        match connect_result {
            Ok(mut controller) => {
                info!("Hardware connected successfully");

                // Test the connection
                if let Err(e) = connection::test_connection(&mut controller).await {
                    warn!("Hardware test failed, but continuing: {}", e);
                }

                info!("Hardware layer ready");

                // Wrap controller in ConnectionManager for automatic reconnection
                let reconnect_config = runtime_config.static_config().reconnect.clone();
                info!(
                    "Reconnection support: {} (max_attempts: {}, initial_delay: {}s, heartbeat: {})",
                    if reconnect_config.enabled { "enabled" } else { "disabled" },
                    if reconnect_config.max_attempts == 0 { "unlimited".to_string() } else { reconnect_config.max_attempts.to_string() },
                    reconnect_config.initial_delay_secs,
                    if reconnect_config.enable_heartbeat { "enabled" } else { "disabled" }
                );

                let manager = Arc::new(ConnectionManager::new(
                    controller,
                    reconnect_config.clone(),
                    timeout_ms,
                    args.verbose,
                ));

                // Start heartbeat task if enabled
                if reconnect_config.enable_heartbeat {
                    info!(
                        "Starting heartbeat monitor (interval: {}s)",
                        reconnect_config.heartbeat_interval_secs
                    );
                    manager.clone().start_heartbeat();
                }

                Some(manager)
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

    // Wrap runtime_config in Arc for sharing between AppState and shutdown handler
    let runtime_config = Arc::new(runtime_config);

    // Clone for shutdown handler (before moving into AppState)
    let runtime_config_for_shutdown = runtime_config.clone();
    let cm_for_shutdown = connection_manager.clone();
    let is_mock = args.mock;

    // Step 5: Create application state with board info
    let app_state = AppState::new(board_info, runtime_config, connection_manager);

    // Set up API router
    let app = api::create_router(app_state);

    // Start server
    info!("Starting server on {}", bind_addr);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;

    info!("OpenFAN API Server listening on {}", bind_addr);
    info!("Server ready!");

    // Run server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            shutdown::apply_safe_boot_profile(
                &runtime_config_for_shutdown,
                cm_for_shutdown.as_ref(),
                is_mock,
            )
            .await;
        })
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

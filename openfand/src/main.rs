//! OpenFAN Server
//!
//! REST API server for controlling fan hardware via serial communication.
//!
//! # Multi-Controller Support
//!
//! Controllers can be specified in two ways:
//!
//! 1. **CLI flags** (single-controller mode): Use `--device` and `--board` to specify
//!    a single controller. This creates an implicit "default" controller.
//!
//! 2. **Config file** (multi-controller mode): Define `[[controllers]]` entries in
//!    the config.toml file for multiple controllers.
//!
//! If `--device` is specified, it takes precedence over config file controllers.

mod api;
mod config;
mod controllers;
mod shutdown;

use anyhow::Result;
use api::AppState;
use clap::Parser;
use config::RuntimeConfig;
use controllers::{connection, ConnectionManager, ControllerEntry, ControllerRegistry};
use openfan_core::{default_config_path, BoardInfo, BoardType};
use std::path::PathBuf;
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
    /// Specifies the board type for single-controller mode.
    /// Use "custom:N" for custom boards with N fans (1-16).
    /// Ignored when [[controllers]] is defined in config.
    #[arg(long, default_value = "standard")]
    board: BoardType,

    /// Serial device path (e.g., /dev/ttyACM0, /dev/ttyUSB0)
    ///
    /// For single-controller mode. Creates an implicit "default" controller.
    /// Takes precedence over [[controllers]] in config file.
    #[arg(long)]
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

    // Step 1: Load configuration
    let runtime_config = RuntimeConfig::load(&config_path).await?;
    info!("Configuration loaded successfully");
    info!("  Static config: {}", config_path.display());
    info!("  Data directory: {}", runtime_config.data_dir().display());

    // Get server config
    let server_config = &runtime_config.static_config().server;
    let port = args.port.unwrap_or(server_config.port);
    let bind_addr = format!("{}:{}", args.bind, port);
    let timeout_ms = server_config.communication_timeout * 1000;
    let reconnect_config = runtime_config.static_config().reconnect.clone();

    // Step 2: Initialize controller registry
    let registry = ControllerRegistry::new();
    let mut default_board_info: Option<BoardInfo> = None;
    let mut default_connection_manager: Option<Arc<ConnectionManager>> = None;

    // Determine controller configuration mode
    if let Some(ref device) = args.device {
        // CLI mode: --device specified, create single "default" controller
        let board_info = args.board.to_board_info();

        info!(
            "Single-controller mode: device={}, board={} ({} fans)",
            device, board_info.name, board_info.fan_count
        );

        let connection_manager = connect_controller(
            "default",
            device,
            timeout_ms,
            args.verbose,
            &reconnect_config,
        )
        .await;

        default_board_info = Some(board_info.clone());
        default_connection_manager = connection_manager.clone();

        let entry = ControllerEntry::builder("default", board_info)
            .maybe_connection_manager(connection_manager)
            .build();
        registry.register(entry).await?;
    } else if !runtime_config.static_config().controllers.is_empty() {
        // Config mode: use [[controllers]] from config file
        let controllers = &runtime_config.static_config().controllers;
        info!(
            "Multi-controller mode: {} controller(s) configured",
            controllers.len()
        );

        for (idx, ctrl_config) in controllers.iter().enumerate() {
            let board_info = ctrl_config.board.to_board_info();

            info!(
                "  Controller '{}': device={}, board={} ({} fans){}",
                ctrl_config.id,
                ctrl_config.device,
                board_info.name,
                board_info.fan_count,
                ctrl_config
                    .description
                    .as_ref()
                    .map(|d| format!(" - {}", d))
                    .unwrap_or_default()
            );

            let connection_manager = if args.mock {
                None
            } else {
                connect_controller(
                    &ctrl_config.id,
                    &ctrl_config.device,
                    timeout_ms,
                    args.verbose,
                    &reconnect_config,
                )
                .await
            };

            // First controller becomes the default for legacy compatibility
            if idx == 0 {
                default_board_info = Some(board_info.clone());
                default_connection_manager = connection_manager.clone();
            }

            let entry = ControllerEntry::builder(&ctrl_config.id, board_info)
                .maybe_connection_manager(connection_manager)
                .maybe_description(ctrl_config.description.clone())
                .build();
            registry.register(entry).await?;
        }
    } else if args.mock {
        // Mock mode without config: create single mock "default" controller
        let board_info = args.board.to_board_info();

        info!(
            "Mock mode: default controller with board={} ({} fans)",
            board_info.name, board_info.fan_count
        );

        default_board_info = Some(board_info.clone());

        let entry = ControllerEntry::builder("default", board_info).build();
        registry.register(entry).await?;
    } else {
        error!(
            "No controllers configured. Use one of:\n  \
             --device /dev/ttyACM0 --board standard    (single controller)\n  \
             --mock --board standard                   (mock mode)\n  \
             Configure [[controllers]] in config.toml  (multi-controller)"
        );
        std::process::exit(1);
    }

    // Ensure we have at least one controller
    let default_board_info =
        default_board_info.expect("At least one controller should be registered");

    info!(
        "Controller registry initialized: {} controller(s)",
        registry.list().await.len()
    );

    // Step 3: Validate global zones against all controllers
    // TODO: In the future, validate each zone fan against its controller's board
    if let Err(e) = runtime_config.validate_for_board(&default_board_info).await {
        error!("Configuration validation failed: {}", e);
        std::process::exit(1);
    }
    info!("Configuration validated successfully");

    // Auto-fill missing defaults for the default controller
    runtime_config
        .fill_defaults_for_board(&default_board_info)
        .await?;

    // Wrap in Arc for sharing
    let registry = Arc::new(registry);
    let runtime_config = Arc::new(runtime_config);

    // Clone for shutdown handler
    let runtime_config_for_shutdown = runtime_config.clone();
    let cm_for_shutdown = default_connection_manager.clone();
    let is_mock = args.mock;

    // Step 4: Create application state
    let app_state = AppState::new(
        registry,
        runtime_config,
        default_board_info,
        default_connection_manager,
    );

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

/// Connect to a single controller and wrap in ConnectionManager
async fn connect_controller(
    id: &str,
    device_path: &str,
    timeout_ms: u64,
    verbose: bool,
    reconnect_config: &openfan_core::ReconnectConfig,
) -> Option<Arc<ConnectionManager>> {
    info!("Connecting to controller '{}' at {}...", id, device_path);

    match connection::connect_to_device(device_path, timeout_ms, verbose).await {
        Ok(mut controller) => {
            info!("Controller '{}' connected successfully", id);

            // Test the connection
            if let Err(e) = connection::test_connection(&mut controller).await {
                warn!(
                    "Controller '{}' hardware test failed, continuing: {}",
                    id, e
                );
            }

            // Wrap in ConnectionManager
            let manager = Arc::new(ConnectionManager::new(
                controller,
                reconnect_config.clone(),
                device_path.to_string(),
                timeout_ms,
                verbose,
            ));

            // Start heartbeat if enabled
            if reconnect_config.enable_heartbeat {
                info!(
                    "Controller '{}': heartbeat enabled (interval: {}s)",
                    id, reconnect_config.heartbeat_interval_secs
                );
                manager.clone().start_heartbeat();
            }

            Some(manager)
        }
        Err(e) => {
            error!(
                "Controller '{}' connection failed: {}. Use --mock for testing without hardware.",
                id, e
            );
            std::process::exit(1);
        }
    }
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

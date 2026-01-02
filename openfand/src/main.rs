//! OpenFAN Server
//!
//! REST API server for controlling fan hardware via serial communication.

mod api;
mod config;
mod hardware;

use anyhow::Result;
use api::AppState;
use clap::Parser;
use config::RuntimeConfig;
use hardware::{connection, ConnectionManager};
use openfan_core::{default_config_path, BoardType, ControlMode};
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

    /// Board type to emulate in mock mode (standard, micro)
    #[arg(long, default_value = "standard", requires = "mock")]
    board: String,
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

        match connection::auto_connect(timeout_ms, args.verbose).await {
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
            apply_safe_boot_profile(
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

/// Apply safe boot profile before shutdown
///
/// Applies a configured fan profile (default: "100% PWM") before the daemon
/// terminates, ensuring fans run at a safe speed during system shutdown/reboot.
///
/// This prevents a thermal safety issue where fans would stop when the daemon
/// terminates but before the system completes shutdown. The profile is applied
/// only if enabled in config and hardware is available.
async fn apply_safe_boot_profile(
    runtime_config: &Arc<RuntimeConfig>,
    connection_manager: Option<&Arc<ConnectionManager>>,
    is_mock: bool,
) {
    let shutdown_config = &runtime_config.static_config().shutdown;

    if !shutdown_config.enabled {
        info!("Safe boot profile disabled in config");
        return;
    }

    if is_mock {
        info!("Mock mode - skipping safe boot profile");
        return;
    }

    let Some(cm) = connection_manager else {
        warn!("No hardware connection - cannot apply safe boot profile");
        return;
    };

    let profile_name = &shutdown_config.profile;
    let profile = {
        let profiles = runtime_config.profiles().await;
        profiles.get(profile_name).cloned()
    };

    let Some(profile) = profile else {
        warn!("Safe boot profile '{}' not found", profile_name);
        return;
    };

    info!("Applying safe boot profile '{}'...", profile_name);

    let result = cm
        .with_controller(|controller| {
            let values = profile.values.clone();
            let mode = profile.control_mode;
            Box::pin(async move {
                for (fan_id, &value) in values.iter().enumerate() {
                    let fan_id = fan_id as u8;
                    let res = match mode {
                        ControlMode::Pwm => controller.set_fan_pwm(fan_id, value).await,
                        ControlMode::Rpm => controller.set_fan_rpm(fan_id, value).await,
                    };
                    if let Err(e) = res {
                        warn!("Failed to set fan {} during shutdown: {}", fan_id, e);
                    }
                }
                Ok(())
            })
        })
        .await;

    match result {
        Ok(_) => info!("Safe boot profile applied successfully"),
        Err(e) => warn!("Failed to apply safe boot profile: {}", e),
    }
}

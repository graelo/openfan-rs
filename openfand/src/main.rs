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
use std::path::PathBuf;
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize tracing
    init_tracing(args.verbose);

    info!("OpenFAN Server starting...");
    info!("Configuration file: {}", args.config.display());

    // Load configuration
    let mut config_manager = ConfigManager::new(&args.config);
    if let Err(e) = config_manager.load().await {
        error!("Failed to load configuration: {}", e);
        return Err(e.into());
    }
    info!("Configuration loaded successfully");

    // Get server config
    let server_config = config_manager.config().server.clone();
    let port = args.port.unwrap_or(server_config.port);
    let bind_addr = format!("{}:{}", args.bind, port);

    // Initialize hardware connection
    let fan_controller = if args.mock {
        info!("Mock mode enabled - running without hardware");
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
                error!("Hardware connection failed: {}", e);
                error!(
                    "Server cannot start without hardware. Use --mock flag to run in mock mode."
                );
                std::process::exit(1);
            }
        }
    };

    // Create application state
    let app_state = AppState::new(config_manager, fan_controller);

    // Set up API router
    let app = api::create_router(app_state);

    // Start server
    info!("Starting server on {}", bind_addr);
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| {
            error!("Failed to bind to {}: {}", bind_addr, e);
            e
        })?;

    info!("OpenFAN API Server listening on {}", bind_addr);
    info!("Phase 3 API layer complete. Server ready!");

    // Run server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| {
            error!("Server error: {}", e);
            e
        })?;

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

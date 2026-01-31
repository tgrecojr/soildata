use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::watch;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use uscrn_ingest::config::Config;
use uscrn_ingest::db::Repository;
use uscrn_ingest::scheduler::Scheduler;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,uscrn_ingest=debug,sqlx=warn")),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("USCRN Data Ingestion Service starting...");

    // Load configuration
    let config = Config::load("config/config.yaml").map_err(|e| {
        anyhow::anyhow!(
            "Failed to load configuration: {}\n\n\
             Make sure:\n\
             1. config/config.yaml exists\n\
             2. All required environment variables are set (check .env.example)\n\
             3. Create a .env file if needed",
            e
        )
    })?;
    info!("Configuration loaded");

    // Connect to database
    let connection_string = config.database.connection_string();
    let pool = PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .connect(&connection_string)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to connect to database: {}\n\n\
                 Host: {}:{}\n\
                 Database: {}\n\
                 User: {}\n\n\
                 Common fixes:\n\
                 1. Ensure PostgreSQL is running\n\
                 2. Check username/password are correct (DB_USER, DB_PASSWORD)\n\
                 3. Verify database exists: createdb {}\n\
                 4. Check host and port (DB_HOST, DB_PORT)",
                e,
                config.database.host,
                config.database.port,
                config.database.name,
                config.database.user,
                config.database.name
            )
        })?;

    info!(
        "Connected to database: {}@{}:{}/{}",
        config.database.user, config.database.host, config.database.port, config.database.name
    );

    // Create repository and run migrations
    let repository = Arc::new(Repository::new(pool));
    repository.run_migrations().await?;

    // Set up shutdown signal
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Spawn signal handler
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });

    // Create and run scheduler
    let mut scheduler = Scheduler::new(config, repository, shutdown_rx);

    if let Err(e) = scheduler.run().await {
        error!("Scheduler error: {}", e);
    }

    info!("USCRN Data Ingestion Service shutting down");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = signal::ctrl_c().await {
            error!("Failed to listen for Ctrl+C: {}", e);
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => {
                error!("Failed to install SIGTERM handler: {}", e);
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, initiating shutdown");
        }
        _ = terminate => {
            info!("Received SIGTERM, initiating shutdown");
        }
    }
}

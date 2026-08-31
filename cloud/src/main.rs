use anyhow::{Context, Result};
use faba_cloud::{AppState, router};
use sqlx::postgres::PgPoolOptions;
use std::{env, net::SocketAddr, path::PathBuf, time::Duration};
use tokio::{net::TcpListener, signal};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "faba_cloud=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    let database_url = env::var("DATABASE_URL").context("DATABASE_URL is required")?;
    let bind_addr = env::var("BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8787".into())
        .parse::<SocketAddr>()
        .context("BIND_ADDR must be a socket address")?;
    let session_days = env::var("SESSION_DAYS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(90)
        .clamp(1, 365);
    let storage_dir =
        PathBuf::from(env::var("AUDIO_STORAGE_DIR").unwrap_or_else(|_| "/data/audio".into()));
    let max_track_bytes = env_u64("MAX_TRACK_BYTES", 200 * 1024 * 1024)
        .clamp(1024 * 1024, 1024 * 1024 * 1024) as usize;
    let max_account_bytes = env_u64("MAX_ACCOUNT_BYTES", 5 * 1024 * 1024 * 1024)
        .clamp(max_track_bytes as u64, 1024 * 1024 * 1024 * 1024);
    let max_total_bytes = env_u64("MAX_TOTAL_BYTES", 50 * 1024 * 1024 * 1024)
        .clamp(max_account_bytes, 1024 * 1024 * 1024 * 1024);
    tokio::fs::create_dir_all(&storage_dir)
        .await
        .context("unable to create audio storage directory")?;

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&database_url)
        .await
        .context("unable to connect to PostgreSQL")?;
    sqlx::migrate!()
        .run(&pool)
        .await
        .context("unable to apply database migrations")?;

    let listener = TcpListener::bind(bind_addr).await?;
    tracing::info!(%bind_addr, "faba cloud listening");
    axum::serve(
        listener,
        router(AppState::new(
            pool,
            session_days,
            storage_dir,
            max_track_bytes,
            max_account_bytes,
            max_total_bytes,
        )),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

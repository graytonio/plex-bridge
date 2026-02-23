mod config;
mod db;
mod error;
mod models;
mod plex;
mod routes;
mod state;
mod sync;

use std::sync::Arc;

use axum::{
    routing::{delete, get, post},
    Router,
};
use clap::Parser;
use tokio::sync::broadcast;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::config::AppConfig;
use crate::db::DbPool;
use crate::state::{AppState, ProgressEvent};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = AppConfig::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("PLEXBRIDGE_LOG")
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("PlexBridge starting on port {}", cfg.port);
    info!("Database: {}", cfg.database_url);

    let db = DbPool::connect(&cfg.database_url).await?;
    db.run_migrations().await?;
    info!("Migrations complete");

    let db = Arc::new(db);

    let max_concurrent = crate::models::config::fetch_config(&db)
        .await?
        .map(|c| c.max_concurrent as usize)
        .unwrap_or(2);

    let (progress_tx, _) = broadcast::channel::<ProgressEvent>(256);
    let (job_tx, job_rx) = tokio::sync::mpsc::channel::<i64>(100);

    let state = AppState::new(db, progress_tx, job_tx);

    let worker_state = state.clone();
    tokio::spawn(async move {
        crate::sync::engine::run_worker_pool(worker_state, job_rx, max_concurrent).await;
    });

    let app = Router::new()
        .route("/", get(routes::dashboard::get_dashboard))
        .route("/clear-completed", post(routes::dashboard::clear_completed))
        .route("/browse", get(routes::browse::get_browse))
        .route("/browse/movies", get(routes::browse::get_movies))
        .route("/browse/shows", get(routes::browse::get_shows))
        .route("/browse/shows/:id/seasons", get(routes::browse::get_show_seasons))
        .route("/queue", post(routes::queue::post_queue))
        .route("/queue/list", get(routes::queue::get_queue_list))
        .route("/queue/stats", get(routes::dashboard::get_queue_stats))
        .route("/queue/:id", delete(routes::queue::delete_queue_item))
        .route("/queue/:id/retry", post(routes::queue::retry_job))
        .route("/settings", get(routes::settings::get_settings))
        .route("/settings", post(routes::settings::post_settings))
        .route("/settings/test", post(routes::settings::test_connection))
        .route("/events", get(routes::events::sse_events))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", cfg.port);
    info!("Listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("PlexBridge shut down");
    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal;

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

    info!("Shutdown signal received");
}

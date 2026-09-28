mod api;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::{
    routing::{get, post},
    Router,
};
use chrono::Utc;
use clap::Parser;
use poly_config::load_from_path;
use poly_store::PostgresStore;
use tower_http::cors::CorsLayer;
use tracing_subscriber::EnvFilter;

use api::{
    analytics_buckets, analytics_daily, analytics_hours, dashboard, events, fills, kill, orders,
    pnl, report_detail, reports, series, status, AppState,
};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "configs/default.toml")]
    config: PathBuf,
    #[arg(long, default_value = "8080")]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();
    let args = Args::parse();
    let cfg = Arc::new(load_from_path(&args.config)?);
    let db = cfg.config().infra.database_url.clone();
    let store = Arc::new(PostgresStore::connect(&db).await?);
    store.migrate().await?;
    let state = AppState {
        store,
        kill_path: cfg.risk().kill_switch_path.clone(),
        config: cfg,
        config_loaded_at: Utc::now(),
    };

    let app = Router::new()
        .route("/api/dashboard", get(dashboard))
        .route("/api/status", get(status))
        .route("/api/pnl", get(pnl))
        .route("/api/reports", get(reports))
        .route("/api/reports/{slug}", get(report_detail))
        .route("/api/events", get(events))
        .route("/api/series", get(series))
        .route("/api/orders", get(orders))
        .route("/api/fills", get(fills))
        .route("/api/analytics/hours", get(analytics_hours))
        .route("/api/analytics/daily", get(analytics_daily))
        .route("/api/analytics/buckets", get(analytics_buckets))
        .route("/api/kill", post(kill))
        .with_state(state)
        .layer(CorsLayer::permissive());

    let addr = format!("0.0.0.0:{}", args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "dashboard-api listening");
    axum::serve(listener, app).await?;
    Ok(())
}

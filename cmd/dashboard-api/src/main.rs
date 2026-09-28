use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use poly_config::load_from_path;
use poly_store::PostgresStore;
use serde::Deserialize;
use sqlx::Row;
use tower_http::cors::CorsLayer;
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
struct AppState {
    store: Arc<PostgresStore>,
    kill_path: String,
}

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
    let cfg = load_from_path(&args.config)?;
    let db = cfg.config().infra.database_url.clone();
    let store = Arc::new(PostgresStore::connect(&db).await?);
    store.migrate().await?;
    let state = AppState {
        store,
        kill_path: cfg.risk().kill_switch_path.clone(),
    };

    let app = Router::new()
        .route("/api/status", get(status))
        .route("/api/pnl", get(pnl))
        .route("/api/reports", get(reports))
        .route("/api/kill", post(kill))
        .with_state(state)
        .layer(CorsLayer::permissive());

    let addr = format!("0.0.0.0:{}", args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "dashboard-api listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn status(State(st): State<AppState>) -> Json<serde_json::Value> {
    let hb = st.store.latest_heartbeat().await.ok().flatten();
    Json(serde_json::json!({ "heartbeat": hb }))
}

async fn pnl(State(st): State<AppState>) -> Json<serde_json::Value> {
    let total = st.store.total_pnl().await.unwrap_or_default();
    let wr = st.store.win_rate().await.unwrap_or(0.0);
    Json(serde_json::json!({ "total_pnl": total.to_string(), "win_rate": wr }))
}

#[derive(Deserialize)]
struct ReportsQuery {
    limit: Option<i64>,
}

async fn reports(
    State(st): State<AppState>,
    Query(q): Query<ReportsQuery>,
) -> Json<serde_json::Value> {
    let limit = q.limit.unwrap_or(50).min(500);
    let rows = sqlx::query(
        "SELECT slug, result, pnl, fees, closed_at FROM market_reports ORDER BY closed_at DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(st.store.pool())
    .await
    .unwrap_or_default();
    let items: Vec<_> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "slug": r.get::<String, _>("slug"),
                "result": r.get::<String, _>("result"),
                "pnl": r.get::<rust_decimal::Decimal, _>("pnl").to_string(),
                "fees": r.get::<rust_decimal::Decimal, _>("fees").to_string(),
                "closed_at": r.get::<chrono::DateTime<chrono::Utc>, _>("closed_at"),
            })
        })
        .collect();
    Json(serde_json::json!({ "reports": items }))
}

async fn kill(State(st): State<AppState>) -> Json<serde_json::Value> {
    if let Some(parent) = std::path::Path::new(&st.kill_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&st.kill_path, b"1");
    Json(serde_json::json!({ "ok": true, "path": st.kill_path }))
}

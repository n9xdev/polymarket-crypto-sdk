use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use poly_store::PostgresStore;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<PostgresStore>,
    pub kill_path: String,
    pub config: std::sync::Arc<poly_config::ConfigHandle>,
    pub config_loaded_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct LimitQuery {
    pub limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct FillsQuery {
    pub limit: Option<i64>,
    pub slug: Option<String>,
}

#[derive(Deserialize)]
pub struct ReportsQuery {
    pub limit: Option<i64>,
    pub asset: Option<String>,
    pub timeframe: Option<String>,
    pub result: Option<String>,
}

#[derive(Deserialize)]
pub struct EventsQuery {
    pub limit: Option<i64>,
    pub kind: Option<String>,
}

#[derive(Deserialize)]
pub struct SeriesQuery {
    pub slug: String,
    pub limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct DailyQuery {
    pub days: Option<i64>,
}

pub async fn dashboard(State(st): State<AppState>) -> Json<Value> {
    let snapshot = st.store.get_snapshot().await.ok().flatten();
    let pnl = st.store.pnl_stats().await.unwrap_or(json!({}));
    let hb = st.store.latest_heartbeat().await.ok().flatten();
    let cfg = st.config.config();
    Json(json!({
        "snapshot": snapshot,
        "heartbeat": hb,
        "pnl": pnl,
        "config_meta": {
            "loaded_at": st.config_loaded_at,
            "path": "configs/default.toml",
            "dry_run": cfg.infra.dry_run,
            "entry_after_sec": cfg.strategy.entry_after_sec,
            "best_ask_min": cfg.strategy.best_ask_min.to_string(),
            "twap_beat_diff_usd": cfg.strategy.twap_beat_diff_usd.to_string(),
            "size_usd": cfg.execution.size_usd.to_string(),
            "order_type": cfg.execution.order_type,
            "max_notional_per_hour": cfg.risk.max_notional_per_hour.to_string(),
            "max_notional_per_day": cfg.risk.max_notional_per_day.to_string(),
            "max_orders_per_market": cfg.strategy.max_orders_per_market,
        },
    }))
}

pub async fn status(State(st): State<AppState>) -> Json<Value> {
    let hb = st.store.latest_heartbeat().await.ok().flatten();
    let snapshot = st.store.get_snapshot().await.ok().flatten();
    Json(json!({ "heartbeat": hb, "snapshot": snapshot }))
}

pub async fn pnl(State(st): State<AppState>) -> Json<Value> {
    Json(st.store.pnl_stats().await.unwrap_or(json!({})))
}

pub async fn reports(State(st): State<AppState>, Query(q): Query<ReportsQuery>) -> Json<Value> {
    let limit = q.limit.unwrap_or(50).min(500);
    let items = st
        .store
        .list_reports_filtered(
            limit,
            q.asset.as_deref(),
            q.timeframe.as_deref(),
            q.result.as_deref(),
        )
        .await
        .unwrap_or_default();
    Json(json!({ "reports": items }))
}

pub async fn report_detail(
    State(st): State<AppState>,
    Path(slug): Path<String>,
) -> Json<Value> {
    let report = st.store.report_by_slug(&slug).await.ok().flatten();
    let series = st.store.slot_series(&slug, 600).await.unwrap_or_default();
    Json(json!({ "report": report, "series": series }))
}

pub async fn events(State(st): State<AppState>, Query(q): Query<EventsQuery>) -> Json<Value> {
    let limit = q.limit.unwrap_or(100).min(1000);
    let items = st
        .store
        .list_events(limit, q.kind.as_deref())
        .await
        .unwrap_or_default();
    Json(json!({ "events": items }))
}

pub async fn series(State(st): State<AppState>, Query(q): Query<SeriesQuery>) -> Json<Value> {
    let limit = q.limit.unwrap_or(360).min(3600);
    let points = st.store.slot_series(&q.slug, limit).await.unwrap_or_default();
    Json(json!({ "slug": q.slug, "points": points }))
}

pub async fn analytics_hours(State(st): State<AppState>) -> Json<Value> {
    Json(json!({ "hours": st.store.analytics_by_hour().await.unwrap_or(json!([])) }))
}

pub async fn analytics_daily(State(st): State<AppState>, Query(q): Query<DailyQuery>) -> Json<Value> {
    let days = q.days.unwrap_or(30).min(365);
    Json(json!({ "days": st.store.analytics_daily(days).await.unwrap_or(json!([])) }))
}

pub async fn analytics_buckets(State(st): State<AppState>) -> Json<Value> {
    Json(json!({ "buckets": st.store.analytics_buckets().await.unwrap_or(json!([])) }))
}

pub async fn orders(State(st): State<AppState>, Query(q): Query<LimitQuery>) -> Json<Value> {
    let limit = q.limit.unwrap_or(50).min(200);
    let rows = sqlx::query(
        "SELECT slug, side, signed_px, size, order_type, status, submitted_at FROM orders ORDER BY submitted_at DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(st.store.pool())
    .await
    .unwrap_or_default();
    let items: Vec<_> = rows
        .iter()
        .map(|r| {
            json!({
                "slug": r.get::<String, _>("slug"),
                "side": r.get::<String, _>("side"),
                "signed_px": r.get::<rust_decimal::Decimal, _>("signed_px").to_string(),
                "size": r.get::<rust_decimal::Decimal, _>("size").to_string(),
                "order_type": r.get::<String, _>("order_type"),
                "status": r.get::<String, _>("status"),
                "submitted_at": r.get::<chrono::DateTime<chrono::Utc>, _>("submitted_at"),
            })
        })
        .collect();
    Json(json!({ "orders": items }))
}

pub async fn fills(State(st): State<AppState>, Query(q): Query<FillsQuery>) -> Json<Value> {
    let limit = q.limit.unwrap_or(50).min(200);
    let rows = if let Some(ref slug) = q.slug {
        sqlx::query(
            "SELECT slug, side, px, size, fee, ts FROM fills WHERE slug = $1 ORDER BY ts ASC LIMIT $2",
        )
        .bind(slug)
        .bind(limit)
        .fetch_all(st.store.pool())
        .await
        .unwrap_or_default()
    } else {
        sqlx::query(
            "SELECT slug, side, px, size, fee, ts FROM fills ORDER BY ts DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(st.store.pool())
        .await
        .unwrap_or_default()
    };
    let items: Vec<_> = rows
        .iter()
        .map(|r| {
            json!({
                "slug": r.get::<String, _>("slug"),
                "side": r.get::<String, _>("side"),
                "px": r.get::<rust_decimal::Decimal, _>("px").to_string(),
                "size": r.get::<rust_decimal::Decimal, _>("size").to_string(),
                "fee": r.get::<rust_decimal::Decimal, _>("fee").to_string(),
                "ts": r.get::<chrono::DateTime<chrono::Utc>, _>("ts"),
            })
        })
        .collect();
    Json(json!({ "fills": items }))
}

pub async fn kill(State(st): State<AppState>) -> Json<Value> {
    if let Some(parent) = std::path::Path::new(&st.kill_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::File::create(&st.kill_path);
    Json(json!({ "ok": true, "path": st.kill_path }))
}

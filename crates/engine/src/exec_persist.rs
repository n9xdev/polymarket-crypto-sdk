use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use poly_domain::{Asset, Timeframe};
use poly_exec::{ExecOutcome, ExecResponse};
use poly_store::{EventKind, EventLog, PostgresStore};
use rust_decimal::Decimal;
use serde_json::json;
use tokio::sync::mpsc;
use tracing::warn;
use uuid::Uuid;

use crate::dashboard::DashboardCtx;

pub fn spawn_exec_persist(
    mut rx: mpsc::UnboundedReceiver<ExecOutcome>,
    store: Arc<PostgresStore>,
    dash_ctx: Arc<DashboardCtx>,
    event_log: EventLog,
) {
    tokio::spawn(async move {
        while let Some(outcome) = rx.recv().await {
            if let Err(e) = persist_one(&store, &dash_ctx, &event_log, outcome).await {
                warn!(error = %e, "order/fill persist failed");
            }
        }
    });
}

async fn persist_one(
    store: &PostgresStore,
    dash_ctx: &DashboardCtx,
    event_log: &EventLog,
    outcome: ExecOutcome,
) -> Result<()> {
    let ExecOutcome { req, resp } = outcome;
    let slug = req.slug.clone();
    let side = req.signed.side.as_str();
    let signed_px = decimal_field(&req.signed.order_json, "price")?;
    let size = decimal_field(&req.signed.order_json, "size")?;

    if let Some((asset, tf)) = slug_asset_tf(&slug) {
        store.upsert_market(&slug, &asset, &tf).await?;
    } else {
        store.upsert_market(&slug, "btc", "5m").await?;
    }

    let (status, clob_order_id, fill_now) = order_status_and_fill(&resp);
    let clob_id = clob_order_id.unwrap_or_else(|| Uuid::new_v4().to_string());

    let order_id = store
        .insert_order(
            &slug,
            side,
            signed_px,
            size,
            &req.order_type,
            Some(clob_id.as_str()),
            status,
            resp.raw.clone(),
        )
        .await?;

    event_log.append(
        EventKind::OrderPost,
        json!({
            "slug": slug,
            "side": side,
            "signed_px": signed_px.to_string(),
            "size": size.to_string(),
            "status": status,
            "clob_order_id": clob_id,
            "dry_run": resp.dry_run,
        }),
    );

    if fill_now {
        let fee = Decimal::ZERO;
        store
            .insert_fill(order_id, &slug, side, signed_px, size, fee)
            .await?;

        let fill_payload = json!({
            "slug": slug,
            "side": side,
            "px": signed_px.to_string(),
            "size": size.to_string(),
            "fee": fee.to_string(),
            "ts": Utc::now().to_rfc3339(),
            "clob_order_id": clob_id,
        });
        dash_ctx.record_fill(fill_payload.clone());
        event_log.append(EventKind::Fill, fill_payload);
    }

    Ok(())
}

/// Paper (dry-run) fills are written immediately with the same status strings as live FAK fills.
fn order_status_and_fill(resp: &ExecResponse) -> (&'static str, Option<String>, bool) {
    if !resp.success {
        return ("failed", None, false);
    }
    if resp.dry_run {
        return ("filled", Some(Uuid::new_v4().to_string()), true);
    }
    let id = extract_clob_order_id(&resp.raw);
    ("submitted", id, false)
}

fn extract_clob_order_id(raw: &serde_json::Value) -> Option<String> {
    for key in ["orderID", "orderId", "id"] {
        if let Some(v) = raw.get(key).and_then(|v| v.as_str()) {
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn decimal_field(order_json: &serde_json::Value, key: &str) -> Result<Decimal> {
    let s = order_json
        .get(key)
        .and_then(|v| v.as_str())
        .context(format!("order_json missing {key}"))?;
    s.parse::<Decimal>()
        .with_context(|| format!("invalid decimal for {key}: {s}"))
}

fn slug_asset_tf(slug: &str) -> Option<(String, String)> {
    let mut parts = slug.split('-');
    let asset = parts.next()?;
    let updown = parts.next()?;
    let tf = parts.next()?;
    if updown != "updown" {
        return None;
    }
    if Asset::parse(asset).is_none() || Timeframe::parse(tf).is_none() {
        return None;
    }
    Some((asset.to_string(), tf.to_string()))
}

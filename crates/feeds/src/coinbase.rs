//! Coinbase Exchange ticker WebSocket → `coinbase_spot` / optional local TWAP60.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use poly_domain::Asset;
use rust_decimal::Decimal;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info};

use crate::bus::{price_slot, SignalBus};
use crate::local_twap::{LocalSpotMetricsConfig, LocalTwap60};

pub struct CoinbaseConfig {
    pub url: String,
    pub assets: Vec<Asset>,
    pub local_metrics: bool,
    pub metrics_cfg: LocalSpotMetricsConfig,
}

pub async fn run_coinbase(cfg: CoinbaseConfig, bus: Arc<SignalBus>) -> Result<()> {
    loop {
        if let Err(e) = run_session(&cfg, bus.clone()).await {
            error!(error = %e, "coinbase ws session ended");
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

async fn run_session(cfg: &CoinbaseConfig, bus: Arc<SignalBus>) -> Result<()> {
    let product_ids: Vec<String> = cfg
        .assets
        .iter()
        .map(|a| format!("{}-USD", a.as_str().to_ascii_uppercase()))
        .collect();
    if product_ids.is_empty() {
        return Ok(());
    }

    let (ws, _) = connect_async(&cfg.url).await?;
    let (mut write, mut read) = ws.split();
    write
        .send(Message::Text(
            serde_json::json!({
                "type": "subscribe",
                "product_ids": product_ids,
                "channels": ["ticker"],
            })
            .to_string()
            .into(),
        ))
        .await?;
    info!(count = product_ids.len(), "coinbase ticker subscribed");

    let mut twap = LocalTwap60::with_config(cfg.metrics_cfg);

    while let Some(msg) = read.next().await {
        let msg = msg?;
        match msg {
            Message::Text(t) => handle_ticker(&t, cfg, &bus, &mut twap),
            Message::Ping(p) => {
                let _ = write.send(Message::Pong(p)).await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    Err(anyhow!("coinbase ws disconnected"))
}

fn handle_ticker(text: &str, cfg: &CoinbaseConfig, bus: &Arc<SignalBus>, twap: &mut LocalTwap60) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };
    if v.get("type").and_then(|x| x.as_str()) != Some("ticker") {
        return;
    }
    let Some(pid) = v.get("product_id").and_then(|x| x.as_str()) else {
        return;
    };
    let Some(asset) = asset_from_coinbase_product(pid) else {
        return;
    };
    if !cfg.assets.contains(&asset) {
        return;
    }
    let Some(px) = v
        .get("price")
        .and_then(|x| x.as_str())
        .and_then(|s| s.parse::<Decimal>().ok())
        .filter(|p| *p > Decimal::ZERO)
    else {
        return;
    };
    let ts_ms = chrono::Utc::now().timestamp_millis();
    bus.coinbase_spot.write(price_slot(px, ts_ms));
    if cfg.local_metrics {
        twap.push(px, ts_ms);
        let (twap_opt, mom_opt) = twap.metrics();
        if let Some((twap_px, twap_ts)) = twap_opt {
            bus.coinbase_twap60.write(price_slot(twap_px, twap_ts));
        }
        if let Some((pct, mom_ts)) = mom_opt {
            bus.coinbase_momentum_pct.write(price_slot(pct, mom_ts));
        }
    }
}

fn asset_from_coinbase_product(product_id: &str) -> Option<Asset> {
    let base = product_id.split('-').next()?;
    Asset::parse(&base.to_lowercase())
}

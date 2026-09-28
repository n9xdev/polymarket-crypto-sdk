//! Binance spot 24hr ticker WebSocket → `binance_spot` / optional local TWAP60.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use poly_domain::Asset;
use rust_decimal::Decimal;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info};

use crate::bus::{price_slot, SignalBus};
use crate::local_twap::LocalTwap60;

pub struct BinanceConfig {
    pub ws_base: String,
    pub assets: Vec<Asset>,
    pub twap60: bool,
}

pub async fn run_binance(cfg: BinanceConfig, bus: Arc<SignalBus>) -> Result<()> {
    loop {
        if let Err(e) = run_session(&cfg, bus.clone()).await {
            error!(error = %e, "binance ws session ended");
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

async fn run_session(cfg: &BinanceConfig, bus: Arc<SignalBus>) -> Result<()> {
    let streams: Vec<String> = cfg
        .assets
        .iter()
        .map(|a| format!("{}usdt@ticker", a.as_str()))
        .collect();
    if streams.is_empty() {
        return Ok(());
    }
    let base = cfg
        .ws_base
        .strip_suffix("/ws")
        .unwrap_or(cfg.ws_base.as_str())
        .trim_end_matches('/');
    let url = format!("{base}/stream?streams={}", streams.join("/"));

    let (ws, _) = connect_async(&url).await?;
    let (_write, mut read) = ws.split();
    info!(count = streams.len(), "binance ticker subscribed");

    let mut twap = LocalTwap60::new();

    while let Some(msg) = read.next().await {
        let msg = msg?;
        if let Message::Text(t) = msg {
            handle_ticker(&t, cfg, &bus, &mut twap);
        }
    }
    Err(anyhow!("binance ws disconnected"))
}

fn handle_ticker(text: &str, cfg: &BinanceConfig, bus: &Arc<SignalBus>, twap: &mut LocalTwap60) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };
    let ticker = if v.get("e").and_then(|x| x.as_str()) == Some("24hrTicker") {
        &v
    } else {
        match v.get("data") {
            Some(d) if d.get("e").and_then(|x| x.as_str()) == Some("24hrTicker") => d,
            _ => return,
        }
    };
    let Some(symbol) = ticker.get("s").and_then(|x| x.as_str()) else {
        return;
    };
    let Some(asset) = asset_from_binance_symbol(symbol) else {
        return;
    };
    if !cfg.assets.contains(&asset) {
        return;
    }
    let Some(px) = ticker
        .get("c")
        .and_then(|x| x.as_str())
        .and_then(|s| s.parse::<Decimal>().ok())
        .filter(|p| *p > Decimal::ZERO)
    else {
        return;
    };
    let ts_ms = chrono::Utc::now().timestamp_millis();
    bus.binance_spot.write(price_slot(px, ts_ms));
    if cfg.twap60 {
        twap.push(px, ts_ms);
        if let Some((twap_px, twap_ts)) = twap.value() {
            bus.binance_twap60.write(price_slot(twap_px, twap_ts));
        }
    }
}

fn asset_from_binance_symbol(symbol: &str) -> Option<Asset> {
    let lower = symbol.to_ascii_lowercase();
    let base = lower.strip_suffix("usdt")?;
    Asset::parse(base)
}

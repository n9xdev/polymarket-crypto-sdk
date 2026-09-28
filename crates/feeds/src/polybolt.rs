use std::sync::Arc;

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use poly_domain::{Asset, PriceSlot};
use rust_decimal::Decimal;
use serde_json::json;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn};

use crate::bus::{price_slot, SignalBus};

pub struct PolyBoltConfig {
    pub url: String,
    pub api_key: String,
    pub api_secret: String,
    pub passphrase: String,
    pub assets: Vec<Asset>,
}

pub async fn run_polybolt(cfg: PolyBoltConfig, bus: Arc<SignalBus>) -> Result<()> {
    loop {
        if let Err(e) = run_session(&cfg, bus.clone()).await {
            error!(error = %e, "polybolt session ended");
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

async fn run_session(cfg: &PolyBoltConfig, bus: Arc<SignalBus>) -> Result<()> {
    let (ws, _) = connect_async(&cfg.url).await?;
    let (mut write, mut read) = ws.split();
    write
        .send(Message::Text(
            json!({
                "op": "auth",
                "rid": "a1",
                "auth": {
                    "apiKey": cfg.api_key,
                    "secret": cfg.api_secret,
                    "passphrase": cfg.passphrase,
                }
            })
            .to_string()
            .into(),
        ))
        .await?;

    let symbols: Vec<String> = cfg
        .assets
        .iter()
        .map(|a| a.polybolt_symbol().to_string())
        .collect();
    let mut subs = vec![];
    for sym in &symbols {
        subs.push(json!({"channel": "price.crypto", "filter": {"symbol": sym}}));
        subs.push(json!({"channel": "price.crypto.twap", "filter": {"symbol": sym, "window_seconds": 60}}));
    }
    write
        .send(Message::Text(
            json!({"op": "subscribe", "rid": "s1", "subscriptions": subs})
                .to_string()
                .into(),
        ))
        .await?;

    info!("polybolt subscribed");

    let ping = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let _ = write.send(Message::Text("PING".into())).await;
        }
    });

    while let Some(msg) = read.next().await {
        let msg = msg?;
        if let Message::Text(t) = msg {
            if t == "PONG" || t == "PING" {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
                handle_envelope(&v, &bus);
            }
        }
    }
    ping.abort();
    Err(anyhow!("polybolt disconnected"))
}

fn handle_envelope(v: &serde_json::Value, bus: &Arc<SignalBus>) {
    let channel = v.get("channel").and_then(|c| c.as_str()).unwrap_or("");
    let payload = v.get("payload").or_else(|| v.get("data"));
    let Some(payload) = payload else { return };
    let ts = payload
        .get("timestamp")
        .or_else(|| v.get("ts"))
        .and_then(|x| x.as_i64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    let px = payload
        .get("full_accuracy_value")
        .and_then(|x| x.as_str())
        .and_then(|s| s.parse::<Decimal>().ok())
        .or_else(|| {
            payload
                .get("value")
                .and_then(|x| x.as_f64())
                .and_then(|f| Decimal::try_from(f).ok())
        });
    let Some(px) = px else {
        warn!("polybolt missing price");
        return;
    };
    match channel {
        "price.crypto" => bus.chainlink_spot.write(price_slot(px, ts)),
        "price.crypto.twap" => bus.chainlink_twap_60.write(price_slot(px, ts)),
        _ => {}
    }
}

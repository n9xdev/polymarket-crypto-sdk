use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use poly_domain::Bbo;
use rust_decimal::Decimal;
use serde_json::json;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info};

use crate::bus::SignalBus;

pub struct ClobBookConfig {
    pub url: String,
    pub token_up: String,
    pub token_down: String,
}

pub async fn run_clob_books(cfg: ClobBookConfig, bus: Arc<SignalBus>) -> Result<()> {
    loop {
        if let Err(e) = run_session(&cfg, bus.clone()).await {
            error!(error = %e, "clob ws ended");
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

async fn run_session(cfg: &ClobBookConfig, bus: Arc<SignalBus>) -> Result<()> {
    let (ws, _) = connect_async(&cfg.url).await?;
    let (mut write, mut read) = ws.split();
    write
        .send(Message::Text(
            json!({
                "type": "market",
                "assets_ids": [cfg.token_up.clone(), cfg.token_down.clone()],
                "custom_feature_enabled": true,
            })
            .to_string()
            .into(),
        ))
        .await?;

    let mut books: HashMap<String, Bbo> = HashMap::new();
    info!("clob market ws subscribed");

    let ping = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
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
                apply_clob_msg(&v, &mut books, &cfg, &bus);
            }
        }
    }
    ping.abort();
    Err(anyhow!("clob ws disconnected"))
}

fn apply_clob_msg(
    v: &serde_json::Value,
    books: &mut HashMap<String, Bbo>,
    cfg: &ClobBookConfig,
    bus: &Arc<SignalBus>,
) {
    let event_type = v.get("event_type").and_then(|x| x.as_str()).unwrap_or("");
    match event_type {
        "best_bid_ask" => {
            let asset = v.get("asset_id").and_then(|x| x.as_str()).unwrap_or("");
            let bid = dec_field(v, "best_bid");
            let ask = dec_field(v, "best_ask");
            let ts = v
                .get("timestamp")
                .and_then(|x| x.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
            let bbo = Bbo {
                bid,
                ask,
                bid_sz: dec_field(v, "best_bid_size"),
                ask_sz: dec_field(v, "best_ask_size"),
                seq: 0,
                src_ts_ms: ts,
                valid: true,
            };
            books.insert(asset.to_string(), bbo);
            publish_bbo(asset, bbo, cfg, bus);
        }
        "price_change" => {
            if let Some(changes) = v.get("price_changes").and_then(|x| x.as_array()) {
                for ch in changes {
                    let asset = ch.get("asset_id").and_then(|x| x.as_str()).unwrap_or("");
                    let entry = books.entry(asset.to_string()).or_default();
                    if let Some(b) = ch.get("best_bid").and_then(|x| x.as_str()) {
                        if let Ok(d) = b.parse::<Decimal>() {
                            entry.bid = d;
                        }
                    }
                    if let Some(a) = ch.get("best_ask").and_then(|x| x.as_str()) {
                        if let Ok(d) = a.parse::<Decimal>() {
                            entry.ask = d;
                        }
                    }
                    entry.valid = true;
                    entry.src_ts_ms = chrono::Utc::now().timestamp_millis();
                    publish_bbo(asset, *entry, cfg, bus);
                }
            }
        }
        _ => {}
    }
}

fn publish_bbo(asset: &str, bbo: Bbo, cfg: &ClobBookConfig, bus: &Arc<SignalBus>) {
    if asset == cfg.token_up {
        bus.book_up.write(bbo);
    } else if asset == cfg.token_down {
        bus.book_down.write(bbo);
    }
}

fn dec_field(v: &serde_json::Value, key: &str) -> Decimal {
    v.get(key)
        .and_then(|x| x.as_str())
        .and_then(|s| s.parse().ok())
        .or_else(|| v.get(key).and_then(|x| x.as_f64()).and_then(|f| Decimal::try_from(f).ok()))
        .unwrap_or_else(|| Decimal::from(0))
}

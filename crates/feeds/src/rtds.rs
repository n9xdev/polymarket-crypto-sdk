use std::sync::Arc;

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use poly_domain::Asset;
use rust_decimal::Decimal;
use serde_json::json;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info};

use crate::bus::{price_slot, SignalBus};

pub struct RtdsConfig {
    pub url: String,
    pub assets: Vec<Asset>,
}

pub async fn run_rtds_twap30(cfg: RtdsConfig, bus: Arc<SignalBus>) -> Result<()> {
    loop {
        if let Err(e) = run_session(&cfg, bus.clone()).await {
            error!(error = %e, "rtds session ended");
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

async fn run_session(cfg: &RtdsConfig, bus: Arc<SignalBus>) -> Result<()> {
    let (ws, _) = connect_async(&cfg.url).await?;
    let (mut write, mut read) = ws.split();
    for asset in &cfg.assets {
        let sym = format!("{}/usd", asset.as_str());
        write
            .send(Message::Text(
                json!({
                    "action": "subscribe",
                    "subscriptions": [{
                        "topic": "crypto_prices_twap_thirty",
                        "type": "*",
                        "filters": json!({"symbol": sym}).to_string(),
                    }]
                })
                .to_string()
                .into(),
            ))
            .await?;
    }
    info!("rtds twap30 subscribed");

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
                let topic = v.get("topic").and_then(|x| x.as_str()).unwrap_or("");
                if topic == "crypto_prices_twap_thirty" {
                    if let Some(payload) = v.get("payload") {
                        let ts = payload
                            .get("timestamp")
                            .and_then(|x| x.as_i64())
                            .unwrap_or(0);
                        let px = payload
                            .get("value")
                            .and_then(|x| x.as_f64())
                            .and_then(|f| Decimal::try_from(f).ok());
                        if let Some(px) = px {
                            bus.chainlink_twap_30.write(price_slot(px, ts));
                        }
                    }
                }
            }
        }
    }
    ping.abort();
    Err(anyhow!("rtds disconnected"))
}

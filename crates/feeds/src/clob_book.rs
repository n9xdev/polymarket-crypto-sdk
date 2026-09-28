//! CLOB market channel: one WS connection, dynamic token subscribe, REST seed.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use poly_domain::Bbo;
use reqwest::Client;
use rust_decimal::Decimal;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{info, warn};

use crate::bus::SignalBus;

pub type TokenSubscribe = Vec<String>;

pub async fn run_clob_book_manager(
    ws_url: String,
    rest_base: String,
    bus: Arc<SignalBus>,
    mut rx: mpsc::UnboundedReceiver<TokenSubscribe>,
) {
    let http = Client::new();
    let mut tokens: HashSet<String> = HashSet::new();

    loop {
        while let Ok(batch) = rx.try_recv() {
            for t in batch {
                tokens.insert(t);
            }
        }

        if tokens.is_empty() {
            tokio::select! {
                batch = rx.recv() => {
                    if let Some(batch) = batch {
                        for t in batch { tokens.insert(t); }
                    } else {
                        return;
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
            }
            continue;
        }

        for t in &tokens {
            match fetch_book_bbo(&http, &rest_base, t).await {
                Ok(bbo) => bus.set_token_book(t, bbo),
                Err(e) => warn!(token = %t, error = %e, "clob REST book seed failed"),
            }
        }

        match run_ws_session(&ws_url, &bus, &tokens, &mut rx).await {
            Ok(()) => info!("clob ws session ended"),
            Err(e) => warn!(error = %e, "clob ws session error"),
        }
        while let Ok(batch) = rx.try_recv() {
            for t in batch {
                tokens.insert(t);
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

async fn run_ws_session(
    ws_url: &str,
    bus: &Arc<SignalBus>,
    tokens: &HashSet<String>,
    rx: &mut mpsc::UnboundedReceiver<TokenSubscribe>,
) -> Result<()> {
    let (ws, _) = connect_async(ws_url).await?;
    let (mut write, mut read) = ws.split();

    let initial: Vec<String> = tokens.iter().cloned().collect();
    write
        .send(Message::Text(
            json!({
                "type": "market",
                "assets_ids": initial,
                "custom_feature_enabled": true,
            })
            .to_string()
            .into(),
        ))
        .await?;
    info!(count = initial.len(), "clob market ws subscribed");
    bus.clob_market_ws_connected
        .store(true, std::sync::atomic::Ordering::Release);

    let ping = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            let _ = write.send(Message::Text("PING".into())).await;
        }
    });

    loop {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(t))) if t != "PONG" && t != "PING" => {
                        apply_clob_text(&t, bus);
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(e.into()),
                    None => break,
                }
            }
            batch = rx.recv() => {
                if batch.is_some() {
                    break;
                }
                return Ok(());
            }
        }
    }

    ping.abort();
    bus.clob_market_ws_connected
        .store(false, std::sync::atomic::Ordering::Release);
    Err(anyhow!("clob ws disconnected"))
}

pub fn apply_clob_text(text: &str, bus: &Arc<SignalBus>) {
    if let Ok(arr) = serde_json::from_str::<Vec<Value>>(text) {
        for v in arr {
            apply_clob_value(&v, bus);
        }
        return;
    }
    if let Ok(v) = serde_json::from_str::<Value>(text) {
        apply_clob_value(&v, bus);
    }
}

fn apply_clob_value(v: &Value, bus: &Arc<SignalBus>) {
    if let Some(payload) = v.get("payload") {
        apply_typed_event(v.get("type").and_then(|x| x.as_str()), payload, bus);
        return;
    }
    let event_type = v.get("event_type").and_then(|x| x.as_str()).unwrap_or("");
    apply_typed_event(Some(event_type), v, bus);
}

fn apply_typed_event(event_type: Option<&str>, v: &Value, bus: &Arc<SignalBus>) {
    match event_type.unwrap_or("") {
        "book" => {
            if let Some(asset) = token_id_from(v) {
                if let Some(bbo) = bbo_from_book_payload(v) {
                    bus.set_token_book(&asset, bbo);
                }
            }
        }
        "best_bid_ask" => {
            if let Some(asset) = token_id_from(v) {
                let bbo = Bbo {
                    bid: dec_field(v, "best_bid"),
                    ask: dec_field(v, "best_ask"),
                    bid_sz: dec_field(v, "best_bid_size"),
                    ask_sz: dec_field(v, "best_ask_size"),
                    seq: 0,
                    src_ts_ms: chrono::Utc::now().timestamp_millis(),
                    valid: true,
                };
                if bbo.bid > Decimal::ZERO || bbo.ask > Decimal::ZERO {
                    bus.set_token_book(&asset, bbo);
                }
            }
        }
        "price_change" => {
            let changes = v
                .get("price_changes")
                .or_else(|| v.get("priceChanges"))
                .and_then(|x| x.as_array());
            if let Some(changes) = changes {
                for ch in changes {
                    let asset = token_id_from(ch).unwrap_or_default();
                    if asset.is_empty() {
                        continue;
                    }
                    let mut bbo = bus.token_book(&asset).unwrap_or_default();
                    if let Some(b) = ch.get("best_bid").or_else(|| ch.get("bestBid")) {
                        bbo.bid = parse_decimal(b);
                    }
                    if let Some(a) = ch.get("best_ask").or_else(|| ch.get("bestAsk")) {
                        bbo.ask = parse_decimal(a);
                    }
                    bbo.valid = bbo.bid > Decimal::ZERO || bbo.ask > Decimal::ZERO;
                    bbo.src_ts_ms = chrono::Utc::now().timestamp_millis();
                    if bbo.valid {
                        bus.set_token_book(&asset, bbo);
                    }
                }
            }
        }
        _ => {}
    }
}

fn token_id_from(v: &Value) -> Option<String> {
    v.get("asset_id")
        .or_else(|| v.get("assetId"))
        .or_else(|| v.get("token_id"))
        .or_else(|| v.get("tokenId"))
        .and_then(|x| x.as_str())
        .map(String::from)
}

fn bbo_from_book_payload(v: &Value) -> Option<Bbo> {
    let bids = v.get("bids")?.as_array()?;
    let asks = v.get("asks")?.as_array()?;
    let mut best_bid = Decimal::ZERO;
    let mut bid_sz = Decimal::ZERO;
    for level in bids {
        let px = level.get("price").map(parse_decimal).unwrap_or_default();
        if px > best_bid {
            best_bid = px;
            bid_sz = level.get("size").map(parse_decimal).unwrap_or_default();
        }
    }
    let mut best_ask = Decimal::ZERO;
    let mut ask_sz = Decimal::ZERO;
    for level in asks {
        let px = level.get("price").map(parse_decimal).unwrap_or_default();
        if px > Decimal::ZERO && (best_ask.is_zero() || px < best_ask) {
            best_ask = px;
            ask_sz = level.get("size").map(parse_decimal).unwrap_or_default();
        }
    }
    if best_bid.is_zero() && best_ask.is_zero() {
        return None;
    }
    Some(Bbo {
        bid: best_bid,
        ask: best_ask,
        bid_sz,
        ask_sz,
        seq: 0,
        src_ts_ms: chrono::Utc::now().timestamp_millis(),
        valid: best_bid > Decimal::ZERO || best_ask > Decimal::ZERO,
    })
}

pub async fn fetch_book_bbo(http: &Client, clob_base: &str, token_id: &str) -> Result<Bbo> {
    let url = format!(
        "{}/book?token_id={}",
        clob_base.trim_end_matches('/'),
        token_id
    );
    let v: Value = http.get(&url).send().await?.json().await?;
    bbo_from_book_payload(&v).ok_or_else(|| anyhow!("empty book for {token_id}"))
}

fn dec_field(v: &Value, key: &str) -> Decimal {
    v.get(key).map(parse_decimal).unwrap_or_default()
}

fn parse_decimal(v: &Value) -> Decimal {
    if let Some(s) = v.as_str() {
        return s.parse().unwrap_or_default();
    }
    v.as_f64()
        .and_then(|f| Decimal::try_from(f).ok())
        .unwrap_or_default()
}

fn ts_field(v: &Value) -> i64 {
    v.get("timestamp")
        .and_then(|x| x.as_str())
        .and_then(|s| s.parse().ok())
        .or_else(|| v.get("timestamp").and_then(|x| x.as_i64()))
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis())
}

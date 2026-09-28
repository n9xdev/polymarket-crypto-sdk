//! Paid Chainlink Data Streams — V3 spot + V2 TWAP 60s.
//!
//! Writes into `SignalBus` slots `chainlink_spot` and `chainlink_twap_60`.
//! Feed IDs: https://data.chain.link/streams

use std::collections::HashMap;
use std::sync::Arc;

use chainlink_data_streams_report::feed_id::ID;
use chainlink_data_streams_report::report::{decode_full_report, v2::ReportDataV2, v3::ReportDataV3};
use chainlink_data_streams_sdk::client::Client;
use chainlink_data_streams_sdk::config::Config as StreamsConfig;
use chainlink_data_streams_sdk::stream::Stream;
use poly_config::ChainlinkDataStreamsConfig;
use rust_decimal::Decimal;
use tokio::time::Duration;
use tracing::{info, warn};

use crate::bus::{price_slot, SignalBus};

const PRICE_DECIMALS: f64 = 1e18;
const WS_BACKOFF_INITIAL: Duration = Duration::from_secs(3);
const WS_BACKOFF_MAX: Duration = Duration::from_secs(30);
const RATE_LIMIT_BACKOFF_INITIAL: Duration = Duration::from_secs(90);
const RATE_LIMIT_BACKOFF_MAX: Duration = Duration::from_secs(600);
const REST_POLL: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, PartialEq, Eq)]
enum StreamKind {
    Spot,
    Twap60,
}

#[derive(Clone)]
struct SubFeed {
    kind: StreamKind,
}

pub struct DataStreamsCredentials {
    pub user_id: String,
    pub secret: String,
}

pub struct DataStreamsConfig {
    pub streams: ChainlinkDataStreamsConfig,
    pub enabled_assets: Vec<String>,
    pub creds: DataStreamsCredentials,
}

pub async fn run_data_streams(cfg: DataStreamsConfig, bus: Arc<SignalBus>) -> anyhow::Result<()> {
    let subs = all_subscriptions(&cfg);
    if subs.is_empty() {
        anyhow::bail!("Chainlink Data Streams: no feed_ids for enabled assets");
    }
    let by_hex = hex_to_sub(&subs);
    let rest_url = cfg.streams.rest_url.clone();
    let ws_url = cfg.streams.ws_url.clone();
    let user_id = cfg.creds.user_id.clone();
    let secret = cfg.creds.secret.clone();

    if let Some(client) = build_rest_client(&user_id, &secret, &rest_url) {
        let poll_bus = bus.clone();
        let poll_subs = subs.clone();
        let poll_map = by_hex.clone();
        tokio::spawn(async move {
            run_rest_poll(client, poll_subs, poll_map, poll_bus).await;
        });
    }

    let streams_cfg = StreamsConfig::new(user_id.clone(), secret.clone(), rest_url, ws_url)
        .build()
        .map_err(|e| anyhow::anyhow!("Chainlink streams config: {e}"))?;

    let mut ids = Vec::new();
    for hex_id in subs.values() {
        if let Ok(id) = ID::from_hex_str(hex_id) {
            ids.push(id);
        }
    }
    if ids.is_empty() {
        anyhow::bail!("Chainlink Data Streams: no valid feed IDs");
    }

    let mut reconnect_delay = WS_BACKOFF_INITIAL;
    loop {
        info!("Chainlink Data Streams WS connecting");
        let mut stream = match Stream::new(&streams_cfg, ids.clone()).await {
            Ok(s) => s,
            Err(e) => {
                let rl = is_rate_limited(&e);
                reconnect_delay = next_reconnect_delay(reconnect_delay, rl);
                warn!(error = %e, "Chainlink connect failed");
                tokio::time::sleep(reconnect_delay).await;
                continue;
            }
        };
        if let Err(e) = stream.listen().await {
            let rl = is_rate_limited(&e);
            reconnect_delay = next_reconnect_delay(reconnect_delay, rl);
            warn!(error = %e, "Chainlink listen failed");
            tokio::time::sleep(reconnect_delay).await;
            continue;
        }

        info!("Chainlink Data Streams connected (spot + TWAP 60s)");
        reconnect_delay = WS_BACKOFF_INITIAL;
        mark_stale_until_data(&bus);

        loop {
            match stream.read().await {
                Ok(ws_report) => {
                    let hex = norm_feed_hex(&ws_report.report.feed_id.to_hex_string());
                    let Some(sub) = by_hex.get(&hex) else {
                        continue;
                    };
                    let Some(price) = decode_report_price(&hex, &ws_report.report.full_report)
                    else {
                        continue;
                    };
                    publish(&bus, sub.kind, price);
                }
                Err(e) => {
                    warn!(error = %e, "Chainlink stream read error");
                    reconnect_delay = next_reconnect_delay(reconnect_delay, is_rate_limited(&e));
                    break;
                }
            }
        }
        tokio::time::sleep(reconnect_delay).await;
    }
}

fn publish(bus: &Arc<SignalBus>, kind: StreamKind, price: f64) {
    let px = Decimal::try_from(price).unwrap_or_else(|_| Decimal::from(0));
    let ts = chrono::Utc::now().timestamp_millis();
    let slot = price_slot(px, ts);
    match kind {
        StreamKind::Spot => bus.chainlink_spot.write(slot),
        StreamKind::Twap60 => bus.chainlink_twap_60.write(slot),
    }
}

fn mark_stale_until_data(bus: &Arc<SignalBus>) {
    let mut spot = bus.chainlink_spot.read();
    spot.valid = false;
    spot.stale = true;
    bus.chainlink_spot.write(spot);
    let mut twap = bus.chainlink_twap_60.read();
    twap.valid = false;
    twap.stale = true;
    bus.chainlink_twap_60.write(twap);
}

fn all_subscriptions(cfg: &DataStreamsConfig) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for asset in &cfg.enabled_assets {
        let key = asset.to_lowercase();
        if let Some(id) = cfg.streams.spot_feed_ids.get(&key) {
            out.insert(format!("spot:{key}"), id.clone());
        }
        if let Some(id) = cfg.streams.twap_60_feed_ids.get(&key) {
            out.insert(format!("twap60:{key}"), id.clone());
        }
    }
    out
}

fn hex_to_sub(subs: &HashMap<String, String>) -> HashMap<String, SubFeed> {
    let mut map = HashMap::new();
    for (key, hex) in subs {
        let kind = if key.starts_with("twap60:") {
            StreamKind::Twap60
        } else if key.starts_with("spot:") {
            StreamKind::Spot
        } else {
            continue;
        };
        map.insert(norm_feed_hex(hex), SubFeed { kind });
    }
    map
}

fn norm_feed_hex(s: &str) -> String {
    s.trim_start_matches("0x")
        .trim_start_matches("0X")
        .to_lowercase()
}

fn decode_report_price(feed_hex: &str, full_report: &str) -> Option<f64> {
    let full_report = full_report.trim_start_matches("0x");
    let bytes = hex::decode(full_report).ok()?;
    let (_ctx, blob) = decode_full_report(&bytes).ok()?;
    let raw = if feed_hex.starts_with("0002") {
        ReportDataV2::decode(&blob)
            .ok()?
            .benchmark_price
            .to_string()
    } else {
        ReportDataV3::decode(&blob)
            .ok()?
            .benchmark_price
            .to_string()
    };
    let price = raw.parse::<f64>().ok()? / PRICE_DECIMALS;
    if price > 0.0 && price.is_finite() {
        Some(price)
    } else {
        None
    }
}

fn build_rest_client(user_id: &str, secret: &str, rest_url: &str) -> Option<Client> {
    let config = StreamsConfig::new(
        user_id.to_string(),
        secret.to_string(),
        rest_url.to_string(),
        "wss://none".to_string(),
    )
    .build()
    .ok()?;
    Client::new(config).ok()
}

async fn run_rest_poll(
    client: Client,
    subs: HashMap<String, String>,
    by_hex: HashMap<String, SubFeed>,
    bus: Arc<SignalBus>,
) {
    let mut interval = tokio::time::interval(REST_POLL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        for hex_id in subs.values() {
            let hex = norm_feed_hex(hex_id);
            let Some(sub) = by_hex.get(&hex) else {
                continue;
            };
            let feed_id = match ID::from_hex_str(hex_id) {
                Ok(id) => id,
                Err(_) => continue,
            };
            match client.get_latest_report(feed_id).await {
                Ok(response) => {
                    if let Some(price) = decode_report_price(&hex, &response.report.full_report) {
                        publish(&bus, sub.kind, price);
                    }
                }
                Err(e) if is_rate_limited(&e) => {
                    warn!(error = %e, "Chainlink REST poll rate limited");
                    break;
                }
                Err(_) => {}
            }
        }
    }
}

fn is_rate_limited(e: &impl std::fmt::Display) -> bool {
    let s = e.to_string().to_lowercase();
    s.contains("429") || s.contains("too many requests")
}

fn next_reconnect_delay(current: Duration, rate_limited: bool) -> Duration {
    if rate_limited {
        if current < RATE_LIMIT_BACKOFF_INITIAL {
            RATE_LIMIT_BACKOFF_INITIAL
        } else {
            (current * 2).min(RATE_LIMIT_BACKOFF_MAX)
        }
    } else {
        (current * 2).min(WS_BACKOFF_MAX)
    }
}

/// REST TWAP at epoch open (optional beat bootstrap).
pub async fn fetch_twap_at_timestamp(
    user_id: &str,
    secret: &str,
    rest_url: &str,
    feed_id_hex: &str,
    epoch_start_unix: i64,
) -> Option<f64> {
    let client = build_rest_client(user_id, secret, rest_url)?;
    let feed_id = ID::from_hex_str(feed_id_hex).ok()?;
    let response = client
        .get_report(feed_id, epoch_start_unix as u128)
        .await
        .ok()?;
    let hex = norm_feed_hex(feed_id_hex);
    decode_report_price(&hex, &response.report.full_report)
}

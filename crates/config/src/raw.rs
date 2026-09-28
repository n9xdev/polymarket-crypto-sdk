use std::collections::HashMap;

use rust_decimal::Decimal;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct RawConfig {
    pub market: RawMarket,
    pub feeds: RawFeeds,
    pub execution: RawExecution,
    pub strategy: RawStrategy,
    pub risk: RawRisk,
    pub infra: RawInfra,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RawMarket {
    pub assets: Vec<String>,
    pub timeframes: Vec<String>,
    #[serde(default)]
    pub slug_tpl: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RawFeeds {
    #[serde(default = "default_true")]
    pub chainlink_spot: bool,
    #[serde(default = "default_polybolt")]
    pub chainlink_source: String,
    pub chainlink_twap: RawChainlinkTwap,
    #[serde(default)]
    pub coinbase: RawOptionalFeed,
    #[serde(default)]
    pub binance: RawOptionalFeed,
    #[serde(default)]
    pub chainlink_rest_url: Option<String>,
    #[serde(default)]
    pub chainlink_ws_url: Option<String>,
    #[serde(default)]
    pub chainlink_spot_feed_ids: HashMap<String, String>,
    #[serde(default)]
    pub chainlink_twap_60_feed_ids: HashMap<String, String>,
}

fn default_true() -> bool {
    true
}

fn default_polybolt() -> String {
    "polybolt".into()
}

#[derive(Debug, Deserialize, Clone)]
pub struct RawChainlinkTwap {
    pub enabled: bool,
    #[serde(default = "default_prefer_60")]
    pub prefer_sec: u32,
}

fn default_prefer_60() -> u32 {
    60
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct RawOptionalFeed {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub twap60: bool,
    #[serde(default)]
    pub ws_url: Option<String>,
}

fn default_coinbase_ws() -> String {
    "wss://ws-feed.exchange.coinbase.com".into()
}

fn default_binance_ws() -> String {
    "wss://stream.binance.com:9443".into()
}

#[derive(Debug, Deserialize, Clone)]
pub struct RawExecution {
    pub order_type: String,
    pub style: String,
    pub size_usd: f64,
    pub grid_min: f64,
    pub grid_max: f64,
    pub grid_step: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RawStrategy {
    pub name: String,
    pub entry_after_sec: u64,
    pub best_ask_min: f64,
    pub twap_beat_diff_usd: f64,
    pub max_orders_per_market: u32,
    pub cooldown_ms: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RawRisk {
    pub max_notional_per_hour: f64,
    pub max_notional_per_day: f64,
    pub kill_switch_path: String,
    #[serde(default = "default_max_ticks")]
    pub max_ticks_through_book: u32,
}

fn default_max_ticks() -> u32 {
    2
}

#[derive(Debug, Deserialize, Clone)]
pub struct RawInfra {
    pub clob: String,
    pub gamma: String,
    #[serde(default)]
    pub polybolt: String,
    #[serde(default)]
    pub rtds: String,
    pub clob_ws: String,
    pub user_ws: String,
    #[serde(default)]
    pub region: String,
    pub dry_run: bool,
    #[serde(default)]
    pub database_url: String,
    #[serde(default)]
    pub redis_url: String,
    #[serde(default = "default_hb")]
    pub heartbeat_interval_ms: u64,
}

fn default_hb() -> u64 {
    1000
}

#[derive(Debug, Clone)]
pub struct MarketConfig {
    pub assets: Vec<String>,
    pub timeframes: Vec<String>,
    pub slug_tpl: String,
}

#[derive(Debug, Clone)]
pub struct FeedsConfig {
    pub chainlink_spot: bool,
    pub chainlink_source: ChainlinkSource,
    pub chainlink_twap: ChainlinkTwapConfig,
    pub chainlink_data_streams: ChainlinkDataStreamsConfig,
    pub coinbase: OptionalFeedConfig,
    pub binance: OptionalFeedConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainlinkSource {
    Polybolt,
    Rtds,
    DataStreams,
}

#[derive(Debug, Clone)]
pub struct ChainlinkDataStreamsConfig {
    pub rest_url: String,
    pub ws_url: String,
    pub spot_feed_ids: HashMap<String, String>,
    pub twap_60_feed_ids: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ChainlinkTwapConfig {
    pub enabled: bool,
    pub prefer_sec: u32,
}

#[derive(Debug, Clone)]
pub struct OptionalFeedConfig {
    pub enabled: bool,
    pub twap60: bool,
    pub ws_url: String,
}

#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    pub order_type: String,
    pub style: String,
    pub size_usd: Decimal,
    pub grid_min: Decimal,
    pub grid_max: Decimal,
    pub grid_step: Decimal,
}

#[derive(Debug, Clone)]
pub struct StrategyConfig {
    pub name: String,
    pub entry_after_sec: u64,
    pub best_ask_min: Decimal,
    pub twap_beat_diff_usd: Decimal,
    pub max_orders_per_market: u32,
    pub cooldown_ms: u64,
}

#[derive(Debug, Clone)]
pub struct RiskConfig {
    pub max_notional_per_hour: Decimal,
    pub max_notional_per_day: Decimal,
    pub kill_switch_path: String,
    pub max_ticks_through_book: u32,
}

#[derive(Debug, Clone)]
pub struct InfraConfig {
    pub clob: String,
    pub gamma: String,
    pub polybolt: String,
    pub rtds: String,
    pub clob_ws: String,
    pub user_ws: String,
    pub region: String,
    pub dry_run: bool,
    pub database_url: String,
    pub redis_url: String,
    pub heartbeat_interval_ms: u64,
}

impl From<RawMarket> for MarketConfig {
    fn from(r: RawMarket) -> Self {
        Self {
            assets: r.assets,
            timeframes: r.timeframes,
            slug_tpl: r
                .slug_tpl
                .unwrap_or_else(|| "{asset}-updown-{tf}-{window_start}".into()),
        }
    }
}

pub fn parse_chainlink_source(s: &str) -> ChainlinkSource {
    match s.to_lowercase().as_str() {
        "rtds" => ChainlinkSource::Rtds,
        "data_streams" | "datastreams" | "chainlink" | "chainlink_streams" => {
            ChainlinkSource::DataStreams
        }
        _ => ChainlinkSource::Polybolt,
    }
}

impl From<RawFeeds> for FeedsConfig {
    fn from(r: RawFeeds) -> Self {
        let chainlink_source = parse_chainlink_source(&r.chainlink_source);
        Self {
            chainlink_spot: r.chainlink_spot,
            chainlink_source,
            chainlink_twap: ChainlinkTwapConfig {
                enabled: r.chainlink_twap.enabled,
                prefer_sec: r.chainlink_twap.prefer_sec,
            },
            chainlink_data_streams: ChainlinkDataStreamsConfig {
                rest_url: r
                    .chainlink_rest_url
                    .unwrap_or_else(|| "https://api.dataengine.chain.link".into()),
                ws_url: r
                    .chainlink_ws_url
                    .unwrap_or_else(|| "wss://ws.dataengine.chain.link".into()),
                spot_feed_ids: r.chainlink_spot_feed_ids,
                twap_60_feed_ids: r.chainlink_twap_60_feed_ids,
            },
            coinbase: OptionalFeedConfig {
                enabled: r.coinbase.enabled,
                twap60: r.coinbase.twap60,
                ws_url: r
                    .coinbase
                    .ws_url
                    .unwrap_or_else(default_coinbase_ws),
            },
            binance: OptionalFeedConfig {
                enabled: r.binance.enabled,
                twap60: r.binance.twap60,
                ws_url: r.binance.ws_url.unwrap_or_else(default_binance_ws),
            },
        }
    }
}

impl From<RawExecution> for ExecutionConfig {
    fn from(r: RawExecution) -> Self {
        Self {
            order_type: r.order_type,
            style: r.style,
            size_usd: dec_f(r.size_usd),
            grid_min: dec_f(r.grid_min),
            grid_max: dec_f(r.grid_max),
            grid_step: dec_f(r.grid_step),
        }
    }
}

impl From<RawStrategy> for StrategyConfig {
    fn from(r: RawStrategy) -> Self {
        Self {
            name: r.name,
            entry_after_sec: r.entry_after_sec,
            best_ask_min: dec_f(r.best_ask_min),
            twap_beat_diff_usd: dec_f(r.twap_beat_diff_usd),
            max_orders_per_market: r.max_orders_per_market,
            cooldown_ms: r.cooldown_ms,
        }
    }
}

impl From<RawRisk> for RiskConfig {
    fn from(r: RawRisk) -> Self {
        Self {
            max_notional_per_hour: dec_f(r.max_notional_per_hour),
            max_notional_per_day: dec_f(r.max_notional_per_day),
            kill_switch_path: r.kill_switch_path,
            max_ticks_through_book: r.max_ticks_through_book,
        }
    }
}

impl From<RawInfra> for InfraConfig {
    fn from(r: RawInfra) -> Self {
        Self {
            clob: r.clob,
            gamma: r.gamma,
            polybolt: r.polybolt,
            rtds: r.rtds,
            clob_ws: r.clob_ws,
            user_ws: r.user_ws,
            region: r.region,
            dry_run: r.dry_run,
            database_url: r.database_url,
            redis_url: r.redis_url,
            heartbeat_interval_ms: r.heartbeat_interval_ms,
        }
    }
}

fn dec_f(v: f64) -> Decimal {
    Decimal::try_from(v).unwrap_or_else(|_| Decimal::from(0))
}

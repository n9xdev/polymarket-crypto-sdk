use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Asset {
    Btc,
    Eth,
    Sol,
    Xrp,
}

impl Asset {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Btc => "btc",
            Self::Eth => "eth",
            Self::Sol => "sol",
            Self::Xrp => "xrp",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "btc" => Some(Self::Btc),
            "eth" => Some(Self::Eth),
            "sol" => Some(Self::Sol),
            "xrp" => Some(Self::Xrp),
            _ => None,
        }
    }

    pub fn polybolt_symbol(&self) -> &'static str {
        match self {
            Self::Btc => "btcusd",
            Self::Eth => "ethusd",
            Self::Sol => "solusd",
            Self::Xrp => "xrpusd",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Timeframe {
    M5,
    M15,
}

impl Timeframe {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::M5 => "5m",
            Self::M15 => "15m",
        }
    }

    pub fn duration_secs(&self) -> i64 {
        match self {
            Self::M5 => 300,
            Self::M15 => 900,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "5m" => Some(Self::M5),
            "15m" => Some(Self::M15),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Slug(pub String);

impl Slug {
    pub fn build(asset: Asset, tf: Timeframe, window_start_unix: i64) -> Self {
        Self(format!(
            "{}-updown-{}-{}",
            asset.as_str(),
            tf.as_str(),
            window_start_unix
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    Up,
    Down,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Up => "Up",
            Self::Down => "Down",
        }
    }

    pub fn other(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketStatus {
    Pending,
    Live,
    Closed,
    Resolved,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub slug: Slug,
    pub asset: Asset,
    pub timeframe: Timeframe,
    pub condition_id: String,
    pub token_up: String,
    pub token_down: String,
    pub tick: Decimal,
    pub neg_risk: bool,
    pub open_ts: DateTime<Utc>,
    pub close_ts: DateTime<Utc>,
    pub twap_lookback_sec: u32,
    /// Price to beat at window open (Chainlink TWAP).
    pub beat: Option<Decimal>,
    pub spot_at_open: Option<Decimal>,
    pub itode: bool,
    pub status: MarketStatus,
    pub fee_rate: Option<Decimal>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Bbo {
    pub bid: Decimal,
    pub ask: Decimal,
    pub bid_sz: Decimal,
    pub ask_sz: Decimal,
    pub seq: u64,
    pub src_ts_ms: i64,
    pub valid: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrderKey {
    pub side: Side,
    pub price_ticks: u32,
}

/// Grid price 0.01 => ticks 1, 0.99 => 99.
pub fn price_to_ticks(px: Decimal) -> u32 {
    (px * Decimal::from(100))
        .round()
        .to_string()
        .parse()
        .unwrap_or(0)
}

pub fn ticks_to_price(ticks: u32) -> Decimal {
    Decimal::from(ticks) / Decimal::from(100)
}

pub fn quantize_to_grid(px: Decimal, step: Decimal) -> Decimal {
    if step.is_zero() {
        return px;
    }
    let steps = (px / step).round();
    steps * step
}

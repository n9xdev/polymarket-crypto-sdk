use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{Bbo, Side};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct PriceSlot {
    pub px: Decimal,
    pub src_ts_ms: i64,
    pub stale: bool,
    pub valid: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StaleFlags {
    pub chainlink_spot: bool,
    pub chainlink_twap: bool,
    pub book_up: bool,
    pub book_down: bool,
    pub binance_spot: bool,
    pub coinbase_spot: bool,
}

impl StaleFlags {
    pub fn any_required(&self) -> bool {
        self.chainlink_spot || self.chainlink_twap || self.book_up || self.book_down
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub engine_ts: DateTime<Utc>,
    pub beat: Option<Decimal>,
    pub chainlink_spot: PriceSlot,
    pub chainlink_twap: PriceSlot,
    pub binance_spot: PriceSlot,
    pub binance_twap60: PriceSlot,
    pub coinbase_spot: PriceSlot,
    pub coinbase_twap60: PriceSlot,
    pub coinbase_momentum_pct: PriceSlot,
    pub binance_momentum_pct: PriceSlot,
    pub up_bbo: Bbo,
    pub down_bbo: Bbo,
    pub secs_into_window: i64,
    pub secs_left: i64,
    pub stale: StaleFlags,
}

impl Signal {
    pub fn bbo(&self, side: Side) -> Bbo {
        match side {
            Side::Up => self.up_bbo,
            Side::Down => self.down_bbo,
        }
    }
}

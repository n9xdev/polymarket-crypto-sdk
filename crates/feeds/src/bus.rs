use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use chrono::Utc;
use poly_domain::{Bbo, PriceSlot, Signal, StaleFlags};
use rust_decimal::Decimal;

const STALE_SPOT_MS: i64 = 2000;
const STALE_BOOK_MS: i64 = 2000;
const STALE_TWAP_MS: i64 = 3000;

#[derive(Debug)]
pub struct LatestSlot<T: Clone + Default> {
    seq: AtomicU64,
    data: RwLock<T>,
}

impl<T: Clone + Default> Default for LatestSlot<T> {
    fn default() -> Self {
        Self {
            seq: AtomicU64::new(0),
            data: RwLock::new(T::default()),
        }
    }
}

impl<T: Clone + Default> LatestSlot<T> {
    pub fn write(&self, value: T) {
        if let Ok(mut g) = self.data.write() {
            *g = value;
            self.seq.fetch_add(1, Ordering::Release);
        }
    }

    pub fn read(&self) -> T {
        let _ = self.seq.load(Ordering::Acquire);
        self.data.read().map(|g| g.clone()).unwrap_or_default()
    }
}

#[derive(Debug, Default)]
pub struct SignalBus {
    pub chainlink_spot: LatestSlot<PriceSlot>,
    pub chainlink_twap_30: LatestSlot<PriceSlot>,
    pub chainlink_twap_60: LatestSlot<PriceSlot>,
    pub binance_spot: LatestSlot<PriceSlot>,
    pub binance_twap60: LatestSlot<PriceSlot>,
    pub coinbase_spot: LatestSlot<PriceSlot>,
    pub coinbase_twap60: LatestSlot<PriceSlot>,
    pub coinbase_momentum_pct: LatestSlot<PriceSlot>,
    pub binance_momentum_pct: LatestSlot<PriceSlot>,
    pub book_up: LatestSlot<Bbo>,
    pub book_down: LatestSlot<Bbo>,
    token_books: RwLock<HashMap<String, Bbo>>,
    pub clob_market_ws_connected: AtomicBool,
    pub beat: LatestSlot<Option<Decimal>>,
}

impl SignalBus {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn set_token_book(&self, token_id: &str, bbo: Bbo) {
        if let Ok(mut g) = self.token_books.write() {
            g.insert(token_id.to_string(), bbo);
        }
    }

    pub fn token_book(&self, token_id: &str) -> Option<Bbo> {
        self.token_books.read().ok()?.get(token_id).copied()
    }

    pub fn any_fresh_token_book(&self, now_ms: i64) -> bool {
        let Ok(guard) = self.token_books.read() else {
            return false;
        };
        guard.values().any(|b| {
            (b.bid > Decimal::ZERO || b.ask > Decimal::ZERO)
                && !is_stale(b.src_ts_ms, now_ms, STALE_BOOK_MS)
        })
    }

    pub fn snapshot(
        &self,
        twap_lookback_sec: u32,
        open_ts_ms: i64,
        close_ts_ms: i64,
        token_up: &str,
        token_down: &str,
    ) -> Signal {
        let now_ms = Utc::now().timestamp_millis();
        let mut spot = self.chainlink_spot.read();
        let twap_slot = if twap_lookback_sec == 30 {
            self.chainlink_twap_30.read()
        } else {
            self.chainlink_twap_60.read()
        };
        let spot_stale = is_stale(spot.src_ts_ms, now_ms, STALE_SPOT_MS) || !spot.valid;
        spot.stale = spot_stale;
        let mut twap = twap_slot;
        let twap_stale = is_stale(twap.src_ts_ms, now_ms, STALE_TWAP_MS) || !twap.valid;
        twap.stale = twap_stale;
        let mut up = self
            .token_book(token_up)
            .unwrap_or_else(|| self.book_up.read());
        let mut down = self
            .token_book(token_down)
            .unwrap_or_else(|| self.book_down.read());
        let up_has_px = up.bid > Decimal::ZERO || up.ask > Decimal::ZERO;
        let down_has_px = down.bid > Decimal::ZERO || down.ask > Decimal::ZERO;
        up.valid = up_has_px && !is_stale(up.src_ts_ms, now_ms, STALE_BOOK_MS);
        down.valid = down_has_px && !is_stale(down.src_ts_ms, now_ms, STALE_BOOK_MS);

        let secs_into = ((now_ms - open_ts_ms).max(0) / 1000) as i64;
        let secs_left = ((close_ts_ms - now_ms).max(0) / 1000) as i64;

        Signal {
            engine_ts: Utc::now(),
            beat: self.beat.read(),
            chainlink_spot: spot,
            chainlink_twap: twap,
            binance_spot: self.mark_stale(self.binance_spot.read(), now_ms, STALE_SPOT_MS),
            binance_twap60: self.mark_stale(self.binance_twap60.read(), now_ms, STALE_TWAP_MS),
            coinbase_spot: self.mark_stale(self.coinbase_spot.read(), now_ms, STALE_SPOT_MS),
            coinbase_twap60: self.mark_stale(self.coinbase_twap60.read(), now_ms, STALE_TWAP_MS),
            coinbase_momentum_pct: self
                .mark_stale(self.coinbase_momentum_pct.read(), now_ms, STALE_SPOT_MS),
            binance_momentum_pct: self
                .mark_stale(self.binance_momentum_pct.read(), now_ms, STALE_SPOT_MS),
            up_bbo: up,
            down_bbo: down,
            secs_into_window: secs_into,
            secs_left,
            stale: StaleFlags {
                chainlink_spot: spot_stale,
                chainlink_twap: twap_stale,
                book_up: !up.valid,
                book_down: !down.valid,
                binance_spot: self.binance_spot.read().stale,
                coinbase_spot: self.coinbase_spot.read().stale,
            },
        }
    }

    fn mark_stale(&self, mut slot: PriceSlot, now_ms: i64, thresh: i64) -> PriceSlot {
        slot.stale = is_stale(slot.src_ts_ms, now_ms, thresh) || !slot.valid;
        slot
    }
}

fn is_stale(src_ts_ms: i64, now_ms: i64, thresh_ms: i64) -> bool {
    src_ts_ms == 0 || now_ms.saturating_sub(src_ts_ms) > thresh_ms
}

pub fn price_slot(px: Decimal, src_ts_ms: i64) -> PriceSlot {
    PriceSlot {
        px,
        src_ts_ms,
        stale: false,
        valid: true,
    }
}

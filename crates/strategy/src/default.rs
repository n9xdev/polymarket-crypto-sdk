use poly_config::StrategyConfig;
use poly_domain::{
    quantize_to_grid, Decision, Market, Side, Signal,
};
use rust_decimal::Decimal;

pub struct DefaultStrategy {
    pub cfg: StrategyConfig,
    pub grid_step: Decimal,
    pub fired_count: u32,
}

impl DefaultStrategy {
    pub fn new(cfg: StrategyConfig, grid_step: Decimal) -> Self {
        Self {
            cfg,
            grid_step,
            fired_count: 0,
        }
    }

    pub fn reset_fired(&mut self) {
        self.fired_count = 0;
    }

    pub fn evaluate(&self, signal: &Signal, market: &Market) -> Decision {
        if signal.secs_into_window < self.cfg.entry_after_sec as i64 {
            return Decision::Hold;
        }
        if self.fired_count >= self.cfg.max_orders_per_market {
            return Decision::Hold;
        }
        if signal.stale.any_required() {
            return Decision::Reject {
                reason: "Stale".into(),
            };
        }
        let beat = match signal.beat.or(market.beat) {
            Some(b) => b,
            None => {
                return Decision::Reject {
                    reason: "NoBeat".into(),
                };
            }
        };
        if !signal.chainlink_twap.valid {
            return Decision::Reject {
                reason: "NoTwap".into(),
            };
        }
        let diff = signal.chainlink_twap.px - beat;
        let side = if diff > self.cfg.twap_beat_diff_usd {
            Side::Up
        } else if diff < -self.cfg.twap_beat_diff_usd {
            Side::Down
        } else {
            return Decision::Hold;
        };
        let bbo = signal.bbo(side);
        if !bbo.valid {
            return Decision::Reject {
                reason: "NoBook".into(),
            };
        }
        if bbo.ask <= self.cfg.best_ask_min {
            return Decision::Hold;
        }
        let px = quantize_to_grid(bbo.ask, self.grid_step);
        Decision::Buy {
            side,
            px,
            reason: format!("twap_diff={diff} beat={beat} ask={}", bbo.ask),
        }
    }
}

impl super::Strategy for DefaultStrategy {
    fn on_signal(&mut self, signal: &Signal, market: &Market) -> Decision {
        self.evaluate(signal, market)
    }

    fn on_market_active(&mut self, _market: &Market) {
        self.reset_fired();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use poly_domain::{Bbo, MarketStatus, PriceSlot, Slug, StaleFlags, Timeframe};
    use rust_decimal_macros::dec;

    fn strat() -> DefaultStrategy {
        DefaultStrategy::new(
            StrategyConfig {
                name: "default".into(),
                entry_after_sec: 240,
                best_ask_min: dec!(0.95),
                twap_beat_diff_usd: dec!(2.0),
                max_orders_per_market: 1,
                cooldown_ms: 250,
            },
            dec!(0.01),
        )
    }

    fn market() -> Market {
        Market {
            slug: Slug("btc-updown-5m-0".into()),
            asset: poly_domain::Asset::Btc,
            timeframe: Timeframe::M5,
            condition_id: "c".into(),
            token_up: "u".into(),
            token_down: "d".into(),
            tick: dec!(0.01),
            neg_risk: false,
            open_ts: Utc::now(),
            close_ts: Utc::now(),
            twap_lookback_sec: 60,
            beat: Some(dec!(100_000)),
            spot_at_open: None,
            itode: true,
            status: MarketStatus::Live,
            fee_rate: None,
        }
    }

    fn signal_base() -> Signal {
        Signal {
            engine_ts: Utc::now(),
            beat: Some(dec!(100_000)),
            chainlink_spot: PriceSlot {
                px: dec!(100_010),
                src_ts_ms: 0,
                stale: false,
                valid: true,
            },
            chainlink_twap: PriceSlot {
                px: dec!(100_003),
                src_ts_ms: 0,
                stale: false,
                valid: true,
            },
            binance_spot: PriceSlot::default(),
            binance_twap60: PriceSlot::default(),
            coinbase_spot: PriceSlot::default(),
            coinbase_twap60: PriceSlot::default(),
            coinbase_momentum_pct: PriceSlot::default(),
            binance_momentum_pct: PriceSlot::default(),
            up_bbo: Bbo {
                bid: dec!(0.94),
                ask: dec!(0.96),
                bid_sz: dec!(100),
                ask_sz: dec!(100),
                seq: 1,
                src_ts_ms: 0,
                valid: true,
            },
            down_bbo: Bbo {
                bid: dec!(0.03),
                ask: dec!(0.04),
                bid_sz: dec!(100),
                ask_sz: dec!(100),
                seq: 1,
                src_ts_ms: 0,
                valid: true,
            },
            secs_into_window: 300,
            secs_left: 0,
            stale: StaleFlags::default(),
        }
    }

    #[test]
    fn picks_up_on_positive_diff() {
        let mut s = signal_base();
        s.chainlink_twap.px = dec!(100_003);
        let d = strat().evaluate(&s, &market());
        match d {
            Decision::Buy { side, .. } => assert_eq!(side, Side::Up),
            _ => panic!("expected buy up"),
        }
    }

    #[test]
    fn picks_down_on_negative_diff() {
        let mut s = signal_base();
        s.chainlink_twap.px = dec!(99_997);
        s.down_bbo.ask = dec!(0.96);
        let d = strat().evaluate(&s, &market());
        match d {
            Decision::Buy { side, .. } => assert_eq!(side, Side::Down),
            _ => panic!("expected buy down"),
        }
    }

    #[test]
    fn hold_when_ask_below_min() {
        let mut s = signal_base();
        s.chainlink_twap.px = dec!(100_010);
        s.up_bbo.ask = dec!(0.94);
        assert!(matches!(strat().evaluate(&s, &market()), Decision::Hold));
    }

    #[test]
    fn reject_stale() {
        let mut s = signal_base();
        s.stale.chainlink_twap = true;
        assert!(matches!(
            strat().evaluate(&s, &market()),
            Decision::Reject { .. }
        ));
    }
}

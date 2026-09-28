use std::collections::VecDeque;
use std::path::Path;

use chrono::{DateTime, Utc};
use poly_config::RiskConfig;
use poly_domain::{Decision, Market, Side, SignedOrder, price_to_ticks};
use rust_decimal::Decimal;

#[derive(Debug, Clone)]
pub enum RiskVerdict {
    Allow,
    Reject(String),
}

#[derive(Debug, Default)]
pub struct RiskGate {
    pub fired_per_slug: std::collections::HashMap<String, u32>,
    pub notional_hour: VecDeque<(DateTime<Utc>, Decimal)>,
    pub notional_day: VecDeque<(DateTime<Utc>, Decimal)>,
    pub cached_usdc: Decimal,
}

impl RiskGate {
    pub fn check(
        &mut self,
        cfg: &RiskConfig,
        market: &Market,
        decision: &Decision,
        signed: Option<&SignedOrder>,
        max_orders: u32,
    ) -> RiskVerdict {
        if Path::new(&cfg.kill_switch_path).exists() {
            return RiskVerdict::Reject("KillSwitch".into());
        }
        let slug = market.slug.0.clone();
        let fired = *self.fired_per_slug.get(&slug).unwrap_or(&0);
        if fired >= max_orders {
            return RiskVerdict::Reject("MaxOrdersPerMarket".into());
        }

        let (side, px, notional) = match decision {
            Decision::Buy { side, px, .. } => {
                let n = cfg.max_notional_per_hour; // placeholder sizing check uses execution size elsewhere
                (*side, *px, n.min(Decimal::from(50)))
            }
            _ => return RiskVerdict::Allow,
        };

        self.evict_old();
        let hour_sum: Decimal = self.notional_hour.iter().map(|(_, v)| *v).sum();
        let day_sum: Decimal = self.notional_day.iter().map(|(_, v)| *v).sum();
        if hour_sum + notional > cfg.max_notional_per_hour {
            return RiskVerdict::Reject("HourlyNotional".into());
        }
        if day_sum + notional > cfg.max_notional_per_day {
            return RiskVerdict::Reject("DailyNotional".into());
        }

        if let Some(order) = signed {
            let expected_token = match side {
                Side::Up => &market.token_up,
                Side::Down => &market.token_down,
            };
            if order.token_id != *expected_token {
                return RiskVerdict::Reject("TokenMismatch".into());
            }
            let ticks = price_to_ticks(px);
            if order.price_ticks != ticks {
                return RiskVerdict::Reject("PriceTicksMismatch".into());
            }
        }

        if self.cached_usdc > Decimal::ZERO && self.cached_usdc < notional {
            return RiskVerdict::Reject("InsufficientCollateral".into());
        }

        let _ = cfg.max_ticks_through_book;
        RiskVerdict::Allow
    }

    pub fn record_fill_notional(&mut self, notional: Decimal) {
        let now = Utc::now();
        self.notional_hour.push_back((now, notional));
        self.notional_day.push_back((now, notional));
    }

    pub fn mark_fired(&mut self, slug: &str) {
        *self.fired_per_slug.entry(slug.to_string()).or_insert(0) += 1;
    }

    fn evict_old(&mut self) {
        let now = Utc::now();
        while self
            .notional_hour
            .front()
            .is_some_and(|(t, _)| now.signed_duration_since(*t).num_hours() >= 1)
        {
            self.notional_hour.pop_front();
        }
        while self
            .notional_day
            .front()
            .is_some_and(|(t, _)| now.signed_duration_since(*t).num_days() >= 1)
        {
            self.notional_day.pop_front();
        }
    }
}

use poly_domain::{OrderKey, Side, SignedOrder, price_to_ticks, ticks_to_price};
use rust_decimal::Decimal;

const MAX_TICKS: usize = 100;

#[derive(Debug)]
pub struct OrderCache {
    up: [Option<SignedOrder>; MAX_TICKS],
    down: [Option<SignedOrder>; MAX_TICKS],
}

impl Default for OrderCache {
    fn default() -> Self {
        Self {
            up: std::array::from_fn(|_| None),
            down: std::array::from_fn(|_| None),
        }
    }
}

impl OrderCache {
    pub fn insert(&mut self, order: SignedOrder) {
        let idx = order.price_ticks as usize;
        if idx >= MAX_TICKS {
            return;
        }
        match order.side {
            Side::Up => self.up[idx] = Some(order),
            Side::Down => self.down[idx] = Some(order),
        }
    }

    pub fn get(&self, key: OrderKey) -> Option<&SignedOrder> {
        let idx = key.price_ticks as usize;
        if idx >= MAX_TICKS {
            return None;
        }
        match key.side {
            Side::Up => self.up[idx].as_ref(),
            Side::Down => self.down[idx].as_ref(),
        }
    }

    pub fn lookup_price(&self, side: Side, px: Decimal) -> Option<&SignedOrder> {
        self.get(OrderKey {
            side,
            price_ticks: price_to_ticks(px),
        })
    }

    pub fn clear(&mut self) {
        self.up = std::array::from_fn(|_| None);
        self.down = std::array::from_fn(|_| None);
    }

    pub fn len_signed(&self) -> usize {
        self.up.iter().filter(|o| o.is_some()).count()
            + self.down.iter().filter(|o| o.is_some()).count()
    }
}

pub fn key_for(side: Side, px: Decimal) -> OrderKey {
    OrderKey {
        side,
        price_ticks: price_to_ticks(px),
    }
}

pub fn px_from_key(key: OrderKey) -> Decimal {
    ticks_to_price(key.price_ticks)
}

#[cfg(test)]
mod presign_tests {
    use poly_config::ExecutionConfig;
    use poly_domain::{Asset, Market, MarketStatus, Slug, Timeframe};
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    use crate::Presigner;

    fn sample_market() -> poly_domain::Market {
        poly_domain::Market {
            slug: Slug("btc-updown-5m-1".into()),
            asset: Asset::Btc,
            timeframe: Timeframe::M5,
            condition_id: "c".into(),
            token_up: "u".into(),
            token_down: "d".into(),
            tick: dec!(0.01),
            neg_risk: false,
            open_ts: chrono::Utc::now(),
            close_ts: chrono::Utc::now(),
            twap_lookback_sec: 60,
            beat: None,
            spot_at_open: None,
            itode: true,
            status: MarketStatus::Pending,
            fee_rate: None,
        }
    }

    #[tokio::test]
    async fn dry_run_presign_198() {
        let grid: Vec<Decimal> = (1..=99).map(|i| Decimal::from(i) / dec!(100)).collect();
        let exec = ExecutionConfig {
            order_type: "FAK".into(),
            style: "LIMIT".into(),
            size_usd: dec!(50),
            grid_min: dec!(0.01),
            grid_max: dec!(0.99),
            grid_step: dec!(0.01),
        };
        let cache = Presigner::new(None)
            .presign_grid(&sample_market(), &exec, &grid, true)
            .await
            .unwrap();
        assert_eq!(cache.len_signed(), 198);
    }
}

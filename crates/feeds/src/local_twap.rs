use std::collections::VecDeque;

use chrono::Utc;
use rust_decimal::Decimal;

#[derive(Debug, Clone, Copy)]
pub struct LocalSpotMetricsConfig {
    pub window_ms: i64,
    pub min_samples: usize,
    pub momentum_lookback_ms: i64,
}

impl LocalSpotMetricsConfig {
    pub fn from_secs(window_sec: u32, min_samples: u32, momentum_lookback_sec: u32) -> Self {
        Self {
            window_ms: i64::from(window_sec) * 1000,
            min_samples: min_samples.max(1) as usize,
            momentum_lookback_ms: i64::from(momentum_lookback_sec) * 1000,
        }
    }
}

#[derive(Debug, Clone)]
struct Sample {
    ts_ms: i64,
    px: Decimal,
}

/// Rolling local TWAP plus % momentum over a lookback window (from ticker stream).
#[derive(Debug)]
pub struct LocalTwap60 {
    cfg: LocalSpotMetricsConfig,
    samples: VecDeque<Sample>,
}

impl Default for LocalTwap60 {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalTwap60 {
    pub fn new() -> Self {
        Self::with_config(LocalSpotMetricsConfig::from_secs(60, 2, 30))
    }

    pub fn with_config(cfg: LocalSpotMetricsConfig) -> Self {
        Self {
            cfg,
            samples: VecDeque::new(),
        }
    }

    pub fn push(&mut self, px: Decimal, ts_ms: i64) {
        self.samples.push_back(Sample { ts_ms, px });
        self.evict(ts_ms);
    }

    fn evict(&mut self, now_ms: i64) {
        let cutoff = now_ms - self.cfg.window_ms;
        while self
            .samples
            .front()
            .is_some_and(|s| s.ts_ms < cutoff)
        {
            self.samples.pop_front();
        }
    }

    /// Time-weighted average over `window_ms` and % change vs price at `now - momentum_lookback_ms`.
    pub fn metrics(&mut self) -> (Option<(Decimal, i64)>, Option<(Decimal, i64)>) {
        self.metrics_at(Utc::now().timestamp_millis())
    }

    pub fn metrics_at(&mut self, now_ms: i64) -> (Option<(Decimal, i64)>, Option<(Decimal, i64)>) {
        self.evict(now_ms);
        (self.twap(now_ms), self.momentum_pct(now_ms))
    }

    fn twap(&self, now_ms: i64) -> Option<(Decimal, i64)> {
        if self.samples.len() < self.cfg.min_samples {
            return self.samples.back().map(|s| (s.px, s.ts_ms));
        }
        let mut weighted = Decimal::ZERO;
        let mut duration = Decimal::ZERO;
        let samples: Vec<_> = self.samples.iter().cloned().collect();
        for w in samples.windows(2) {
            let dt = Decimal::from(w[1].ts_ms - w[0].ts_ms);
            if dt > Decimal::ZERO {
                weighted += w[0].px * dt;
                duration += dt;
            }
        }
        if duration.is_zero() {
            return samples.last().map(|s| (s.px, s.ts_ms));
        }
        Some((weighted / duration, now_ms))
    }

    fn momentum_pct(&self, now_ms: i64) -> Option<(Decimal, i64)> {
        let current = self.samples.back()?;
        if current.px <= Decimal::ZERO {
            return None;
        }
        let target_ts = now_ms - self.cfg.momentum_lookback_ms;
        let ref_px = self.price_at_or_before(target_ts)?;
        if ref_px <= Decimal::ZERO {
            return None;
        }
        let hundred = Decimal::from(100);
        let pct = (current.px - ref_px) / ref_px * hundred;
        Some((pct, now_ms))
    }

    /// Latest sample at or before `ts_ms`.
    fn price_at_or_before(&self, ts_ms: i64) -> Option<Decimal> {
        self.samples
            .iter()
            .rev()
            .find(|s| s.ts_ms <= ts_ms)
            .map(|s| s.px)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn momentum_pct_over_lookback() {
        let cfg = LocalSpotMetricsConfig::from_secs(60, 1, 10);
        let mut m = LocalTwap60::with_config(cfg);
        m.push(d("100"), 0);
        m.push(d("101"), 5_000);
        m.push(d("102"), 10_000);
        let (_, mom) = m.metrics_at(10_000);
        let (pct, _) = mom.expect("momentum");
        assert_eq!(pct, d("2"));
    }
}

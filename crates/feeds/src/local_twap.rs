use std::collections::VecDeque;

use chrono::Utc;
use rust_decimal::Decimal;

#[derive(Debug, Clone)]
struct Sample {
    ts_ms: i64,
    px: Decimal,
}

/// Rolling 60s time-weighted average (local, not settlement).
#[derive(Debug, Default)]
pub struct LocalTwap60 {
    window_ms: i64,
    samples: VecDeque<Sample>,
}

impl LocalTwap60 {
    pub fn new() -> Self {
        Self {
            window_ms: 60_000,
            samples: VecDeque::new(),
        }
    }

    pub fn push(&mut self, px: Decimal, ts_ms: i64) {
        self.samples.push_back(Sample { ts_ms, px });
        self.evict(ts_ms);
    }

    fn evict(&mut self, now_ms: i64) {
        let cutoff = now_ms - self.window_ms;
        while self
            .samples
            .front()
            .is_some_and(|s| s.ts_ms < cutoff)
        {
            self.samples.pop_front();
        }
    }

    pub fn value(&mut self) -> Option<(Decimal, i64)> {
        let now_ms = Utc::now().timestamp_millis();
        self.evict(now_ms);
        if self.samples.len() < 2 {
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
}

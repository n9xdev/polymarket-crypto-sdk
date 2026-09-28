use chrono::{DateTime, TimeZone, Utc};
use poly_domain::{Asset, Slug, Timeframe};

#[derive(Debug, Clone)]
pub struct WindowSlug {
    pub slug: Slug,
    pub window_start: i64,
    pub window_end: i64,
    pub asset: Asset,
    pub timeframe: Timeframe,
}

pub fn floor_window_start(adjusted_unix: i64, tf: Timeframe) -> i64 {
    let w = tf.duration_secs();
    adjusted_unix - (adjusted_unix % w)
}

pub fn compute_window_slugs(
    asset: Asset,
    tf: Timeframe,
    adjusted_unix: i64,
) -> (WindowSlug, WindowSlug) {
    let start = floor_window_start(adjusted_unix, tf);
    let end = start + tf.duration_secs();
    let next_start = end;
    let next_end = next_start + tf.duration_secs();
    let current = WindowSlug {
        slug: Slug::build(asset, tf, start),
        window_start: start,
        window_end: end,
        asset,
        timeframe: tf,
    };
    let next = WindowSlug {
        slug: Slug::build(asset, tf, next_start),
        window_start: next_start,
        window_end: next_end,
        asset,
        timeframe: tf,
    };
    (current, next)
}

pub fn window_start_dt(ts: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(ts, 0).single().unwrap_or_else(Utc::now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_format() {
        let (cur, _) = compute_window_slugs(Asset::Btc, Timeframe::M5, 1_700_000_000);
        assert!(cur.slug.0.starts_with("btc-updown-5m-"));
    }
}

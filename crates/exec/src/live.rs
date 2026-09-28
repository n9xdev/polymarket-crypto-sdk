//! Live-only helpers (reconcile, alerts) — wired when dry_run is false.

use tracing::warn;

pub fn alert_stale_feed(name: &str, age_ms: i64) {
    if age_ms > 5000 {
        warn!(feed = name, age_ms, "required feed stale > 5s");
    }
}

pub fn alert_kill_switch(active: bool) {
    if active {
        warn!("kill switch active");
    }
}

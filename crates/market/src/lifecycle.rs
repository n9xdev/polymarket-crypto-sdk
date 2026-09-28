use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use poly_config::{ConfigHandle, InfraConfig};
use poly_domain::{Asset, Market, MarketStatus, Slug, Timeframe};
use rust_decimal::Decimal;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::gamma::{gamma_to_market, GammaClient};
use crate::slug::{compute_window_slugs, window_start_dt};
use crate::time_sync::{adjusted_unix, measure_server_offset};

#[derive(Debug, Clone)]
pub enum LifecycleEvent {
    MarketReady {
        market: Arc<Market>,
    },
    /// Emitted once when wall-clock enters [open, close) for a prefetched market.
    MarketActive {
        market: Arc<Market>,
    },
    MarketClosed {
        slug: Slug,
    },
    TickSizeChange {
        slug: Slug,
        tick: Decimal,
    },
    BeatCaptured {
        slug: Slug,
        beat: Decimal,
        spot: Option<Decimal>,
    },
}

pub struct MarketLifecycle {
    cfg: ConfigHandle,
    gamma: GammaClient,
    server_offset: f64,
    tx: mpsc::UnboundedSender<LifecycleEvent>,
}

impl MarketLifecycle {
    pub fn new(cfg: ConfigHandle, tx: mpsc::UnboundedSender<LifecycleEvent>) -> Self {
        let infra = cfg.config().infra.clone();
        Self {
            gamma: GammaClient::new(infra.gamma),
            cfg,
            server_offset: 0.0,
            tx,
        }
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        let infra = self.cfg.config().infra.clone();
        if let Ok(off) = measure_server_offset(&infra.clob).await {
            self.server_offset = off;
            info!(offset = off, "clob time sync");
        }

        let mut tracked: HashMap<String, Arc<Market>> = HashMap::new();
        let mut live_notified: HashSet<String> = HashSet::new();
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            self.tick(&infra, &mut tracked, &mut live_notified).await?;
        }
    }

    async fn tick(
        &self,
        infra: &InfraConfig,
        tracked: &mut HashMap<String, Arc<Market>>,
        live_notified: &mut HashSet<String>,
    ) -> anyhow::Result<()> {
        let cfg = self.cfg.config();
        let now = adjusted_unix(Utc::now().timestamp(), self.server_offset);

        for asset_str in &cfg.market.assets {
            let asset = Asset::parse(asset_str).unwrap();
            for tf_str in &cfg.market.timeframes {
                let tf = Timeframe::parse(tf_str).unwrap();
                let (current, next) = compute_window_slugs(asset, tf, now);
                for ws in [&current, &next] {
                    let slug_key = ws.slug.0.clone();
                    let secs_to_open = ws.window_start - now;
                    if secs_to_open > 60 {
                        continue;
                    }
                    if tracked.contains_key(&slug_key) {
                        continue;
                    }
                    match self.gamma.fetch_by_slug(&ws.slug.0).await {
                        Ok(raw) => {
                            let itode = self
                                .gamma
                                .fetch_itode(&infra.clob, raw.condition_id.as_deref().unwrap_or(""))
                                .await
                                .unwrap_or(false);
                            let mut market =
                                gamma_to_market(&raw, ws.slug.clone(), asset, tf, itode)?;
                            market.open_ts = window_start_dt(ws.window_start);
                            market.close_ts = window_start_dt(ws.window_end);
                            let arc = Arc::new(market);
                            tracked.insert(slug_key.clone(), arc.clone());
                            let _ = self.tx.send(LifecycleEvent::MarketReady { market: arc });
                            info!(slug = %slug_key, "market ready prefetch");
                        }
                        Err(e) => warn!(slug = %ws.slug.0, error = %e, "gamma fetch failed"),
                    }
                }

            }
        }

        self.close_expired(tracked, live_notified, now);
        self.notify_active(tracked, live_notified, now);
        Ok(())
    }

    fn close_expired(
        &self,
        tracked: &mut HashMap<String, Arc<Market>>,
        live_notified: &mut HashSet<String>,
        now: i64,
    ) {
        let expired: Vec<String> = tracked
            .iter()
            .filter(|(_, m)| now >= m.close_ts.timestamp())
            .map(|(k, _)| k.clone())
            .collect();
        for key in expired {
            if let Some(m) = tracked.remove(&key) {
                live_notified.remove(&key);
                let _ = self
                    .tx
                    .send(LifecycleEvent::MarketClosed { slug: m.slug.clone() });
            }
        }
    }

    fn notify_active(
        &self,
        tracked: &HashMap<String, Arc<Market>>,
        live_notified: &mut HashSet<String>,
        now: i64,
    ) {
        for (key, m) in tracked {
            if now >= m.open_ts.timestamp()
                && now < m.close_ts.timestamp()
                && !live_notified.contains(key)
            {
                live_notified.insert(key.clone());
                let _ = self.tx.send(LifecycleEvent::MarketActive {
                    market: m.clone(),
                });
                info!(slug = %key, "market active");
            }
        }
    }
}

/// Called when TWAP sample arrives at/after open.
pub fn try_capture_beat(
    market: &mut Market,
    twap_px: Decimal,
    twap_ts_ms: i64,
    spot_px: Option<Decimal>,
) -> Option<(Decimal, Option<Decimal>)> {
    if market.beat.is_some() {
        return None;
    }
    let open_ms = market.open_ts.timestamp_millis();
    if twap_ts_ms + 5000 < open_ms {
        return None;
    }
    if twap_ts_ms < open_ms.saturating_sub(2000) {
        return None;
    }
    market.beat = Some(twap_px);
    market.spot_at_open = spot_px;
    market.status = MarketStatus::Live;
    Some((twap_px, spot_px))
}

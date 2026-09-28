use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use poly_config::{ChainlinkSource, ConfigHandle};
use poly_domain::{Decision, Market, Side};
use poly_exec::{
    alert_kill_switch, alert_stale_feed, spawn_executor_worker, ExecRequest, Executor,
};
use poly_exec::{run_user_ws, UserWsConfig};
use poly_feeds::FeedHub;
use poly_market::{try_capture_beat, LifecycleEvent, MarketLifecycle};
use poly_risk::{RiskGate, RiskVerdict};
use poly_settle::{build_report, fetch_outcome_up_won, spawn_redeem_timer};
use poly_signer::{OrderCache, Presigner};
use poly_store::{EventKind, EventLog, spawn_writer, PostgresStore};
use poly_strategy::Strategy;
use rust_decimal::Decimal;
use tokio::sync::mpsc;
use tracing::{info, warn};

fn parse_fill_side(s: &str) -> Option<Side> {
    match s {
        "Up" => Some(Side::Up),
        "Down" => Some(Side::Down),
        _ => None,
    }
}

use crate::beat::fetch_beat_at_window_open;
use crate::dashboard::{build_snapshot, slot_sample_payload, DashboardCtx};
use crate::exec_persist::spawn_exec_persist;
use crate::report_backfill::backfill_paper_reports;
use crate::secrets::{load_dotenv, EngineSecrets};

pub struct Engine;

impl Engine {
    pub async fn run(cfg: ConfigHandle, mut strategy: Box<dyn Strategy>) -> Result<()> {
        load_dotenv();
        let static_cfg = cfg.config();
        let secrets = EngineSecrets::from_env(&static_cfg.infra)?;
        EngineSecrets::require_chainlink_streams(&static_cfg)?;

        let store = match PostgresStore::connect(&static_cfg.infra.database_url).await {
            Ok(s) => {
                if let Err(e) = s.migrate().await {
                    warn!(error = %e, "postgres migrate failed — continuing without durable store");
                    None
                } else {
                    Some(Arc::new(s))
                }
            }
            Err(e) => {
                warn!(error = %e, "postgres unavailable — continuing without durable store");
                None
            }
        };

        let (event_log, event_rx) = EventLog::new();
        if let Some(ref st) = store {
            spawn_writer(event_rx, st.clone());
            backfill_paper_reports(st.as_ref(), static_cfg.infra.dry_run).await;
        }

        let (life_tx, mut life_rx) = mpsc::unbounded_channel();
        let lifecycle = MarketLifecycle::new(cfg.clone(), life_tx);
        tokio::spawn(async move {
            if let Err(e) = lifecycle.run().await {
                warn!(error = %e, "lifecycle exited");
            }
        });

        let mut hub = FeedHub::new();
        hub.spawn_data_streams(
            cfg.clone(),
            secrets.chainlink_streams_user_id.clone(),
            secrets.chainlink_streams_secret.clone(),
        );
        hub.spawn_polybolt(
            cfg.clone(),
            secrets.api_key.clone(),
            secrets.api_secret.clone(),
            secrets.passphrase.clone(),
        );
        hub.spawn_rtds_30(cfg.clone());
        hub.spawn_coinbase(cfg.clone());
        hub.spawn_binance(cfg.clone());

        let (exec_req_tx, exec_req_rx) = mpsc::unbounded_channel();
        let (exec_log_tx, exec_log_rx) = mpsc::unbounded_channel();
        let executor = Executor::new(
            static_cfg.infra.clone(),
            secrets.api_key.clone(),
            secrets.api_secret.clone(),
            secrets.passphrase.clone(),
            secrets.address.clone(),
            Some(exec_log_tx),
        );
        executor.warm_tls().await?;
        spawn_executor_worker(executor.clone(), exec_req_rx);

        let dash_ctx = Arc::new(DashboardCtx::new());
        if let Some(ref st) = store {
            spawn_exec_persist(
                exec_log_rx,
                st.clone(),
                dash_ctx.clone(),
                event_log.clone(),
            );
        }

        if !static_cfg.infra.dry_run {
            let user_cfg = UserWsConfig {
                url: static_cfg.infra.user_ws.clone(),
                api_key: secrets.api_key.clone(),
                api_secret: secrets.api_secret.clone(),
                passphrase: secrets.passphrase.clone(),
                address: secrets.address.clone(),
            };
            tokio::spawn(async move {
                let _ = run_user_ws(user_cfg).await;
            });
            spawn_redeem_timer(false);
        } else {
            spawn_redeem_timer(true);
        }

        let presigner = Presigner::new(secrets.private_key.clone());
        let cache: Arc<Mutex<HashMap<String, OrderCache>>> = Arc::new(Mutex::new(HashMap::new()));
        let mut risk = RiskGate::default();
        let mut active_markets: HashMap<String, Arc<Market>> = HashMap::new();
        let bus = hub.bus.clone();
        let grid = static_cfg.grid_prices.clone();
        let exec_cfg = static_cfg.execution.clone();
        let dry_run = static_cfg.infra.dry_run;

        let mut fired_slugs: HashMap<String, ()> = HashMap::new();
        let mut last_fire_ms: i64 = 0;
        let mut last_slot_sample_ms: HashMap<String, i64> = HashMap::new();
        let mut last_beat_fetch_ms: HashMap<String, i64> = HashMap::new();
        let secrets_for_beat = secrets.clone();

        let mut interval = tokio::time::interval(Duration::from_millis(1));
        let mut heartbeat = tokio::time::interval(Duration::from_millis(
            static_cfg.infra.heartbeat_interval_ms.max(1000),
        ));

        loop {
            tokio::select! {
                Some(ev) = life_rx.recv() => {
                    match ev {
                        LifecycleEvent::MarketReady { market } => {
                            hub.spawn_clob_books(cfg.clone(), market.token_up.clone(), market.token_down.clone());
                            let slug = market.slug.0.clone();
                            match presigner.presign_grid(&market, &exec_cfg, &grid, dry_run).await {
                                Ok(c) => {
                                    cache.lock().unwrap().insert(slug.clone(), c);
                                    info!(slug = %slug, "presign complete");
                                }
                                Err(e) => warn!(error = %e, "presign failed"),
                            }
                        }
                        LifecycleEvent::MarketActive { market } => {
                            hub.spawn_clob_books(
                                cfg.clone(),
                                market.token_up.clone(),
                                market.token_down.clone(),
                            );
                            let tf_key = market.timeframe.as_str().to_string();
                            active_markets.insert(tf_key.clone(), market.clone());
                            fired_slugs.remove(&market.slug.0);
                            strategy.on_market_active(&market);
                            if let Some(ref st) = store {
                                let _ = st
                                    .upsert_market(
                                        &market.slug.0,
                                        market.asset.as_str(),
                                        market.timeframe.as_str(),
                                    )
                                    .await;
                            }
                            info!(slug = %market.slug.0, tf = %tf_key, "market active");
                        }
                        LifecycleEvent::MarketClosed { slug } => {
                            cache.lock().unwrap().remove(&slug.0);
                            fired_slugs.remove(&slug.0);
                            let closed_key = active_markets
                                .iter()
                                .find(|(_, m)| m.slug == slug)
                                .map(|(k, _)| k.clone());
                            if let Some(key) = closed_key {
                                if let Some(m) = active_markets.remove(&key) {
                                    let close_twap = bus.chainlink_twap_60.read().px;
                                    let official = fetch_outcome_up_won(&static_cfg.infra.gamma, &m.slug.0).await.ok().flatten();
                                    let trade = if let Some(ref st) = store {
                                        st.primary_fill_for_slug(&m.slug.0).await.ok().flatten()
                                    } else {
                                        None
                                    };
                                    let (side, signed_px, share_size, fill_fee) = match trade {
                                        Some((s, px, size, fee)) => {
                                            (parse_fill_side(&s), Some(px), size, Some(fee))
                                        }
                                        None => (None, None, Decimal::ZERO, None),
                                    };
                                    let report = build_report(
                                        &m,
                                        &m.slug.0,
                                        side,
                                        signed_px,
                                        signed_px,
                                        share_size,
                                        Some(close_twap),
                                        Some(bus.chainlink_spot.read().px),
                                        None,
                                        official,
                                        fill_fee,
                                        dry_run,
                                    );
                                    if let Some(ref st) = store {
                                        let _ = st.write_report(&report).await;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ = heartbeat.tick() => {
                    let kill = Path::new(&cfg.risk().kill_switch_path).exists();
                    let spot_age =
                        chrono::Utc::now().timestamp_millis() - bus.chainlink_spot.read().src_ts_ms;
                    let twap_age =
                        chrono::Utc::now().timestamp_millis() - bus.chainlink_twap_60.read().src_ts_ms;
                    alert_stale_feed("chainlink_spot", spot_age);
                    alert_stale_feed("chainlink_twap", twap_age);
                    let slot_ages = serde_json::json!({ "spot": spot_age, "twap": twap_age });
                    event_log.append(EventKind::Heartbeat, slot_ages.clone());
                    alert_kill_switch(kill);
                    if let Some(ref st) = store {
                        let primary_slug = active_markets.get("5m")
                            .or_else(|| active_markets.values().next())
                            .map(|m| m.slug.0.as_str());
                        let _ = st.write_heartbeat(
                            primary_slug,
                            slot_ages,
                            kill,
                            None,
                            dry_run,
                        ).await;
                        let snap = build_snapshot(
                            &cfg,
                            &bus,
                            &active_markets,
                            &cache,
                            &risk,
                            &dash_ctx,
                            dry_run,
                            kill,
                            "5m",
                        );
                        let _ = st.upsert_snapshot(snap).await;
                    }
                }
                _ = interval.tick() => {
                    if active_markets.is_empty() {
                        continue;
                    }
                    let now_ms = chrono::Utc::now().timestamp_millis();
                    let keys: Vec<String> = active_markets.keys().cloned().collect();
                    for tf_key in keys {
                        let Some(market) = active_markets.get(&tf_key).cloned() else { continue };
                        let mut m = (*market).clone();
                        let open_ms = m.open_ts.timestamp_millis();
                        let close_ms = m.close_ts.timestamp_millis();

                        if static_cfg.feeds.chainlink_source == ChainlinkSource::DataStreams {
                            if now_ms >= open_ms && now_ms < close_ms {
                                let last_fetch = last_beat_fetch_ms.get(&m.slug.0).copied().unwrap_or(0);
                                if now_ms.saturating_sub(last_fetch) >= 5000 {
                                    last_beat_fetch_ms.insert(m.slug.0.clone(), now_ms);
                                    if let Some(beat) =
                                        fetch_beat_at_window_open(&cfg, &secrets_for_beat, &m).await
                                    {
                                        if m.beat != Some(beat) {
                                            m.beat = Some(beat);
                                            m.spot_at_open = Some(bus.chainlink_spot.read().px);
                                            bus.beat.write(Some(beat));
                                            active_markets.insert(tf_key.clone(), Arc::new(m.clone()));
                                        }
                                    }
                                }
                            }
                        } else if m.beat.is_none() {
                            let twap = bus.chainlink_twap_60.read();
                            if twap.valid
                                && try_capture_beat(
                                    &mut m,
                                    twap.px,
                                    twap.src_ts_ms,
                                    Some(bus.chainlink_spot.read().px),
                                )
                                .is_some()
                            {
                                active_markets.insert(tf_key.clone(), Arc::new(m.clone()));
                            }
                        }

                        let mut signal = bus.snapshot(
                            m.twap_lookback_sec,
                            open_ms,
                            close_ms,
                            &m.token_up,
                            &m.token_down,
                        );
                        signal.beat = m.beat.or(signal.beat);

                        let decision = strategy.on_signal(&signal, &m);
                        dash_ctx.record_decision(&m.slug.0, &decision);
                        if !matches!(decision, Decision::Hold) {
                            event_log.append(
                                EventKind::Decision,
                                serde_json::json!({ "slug": m.slug.0, "decision": format!("{decision:?}") }),
                            );
                        }

                        let last = last_slot_sample_ms.get(&m.slug.0).copied().unwrap_or(0);
                        if now_ms - last >= 1000 {
                            last_slot_sample_ms.insert(m.slug.0.clone(), now_ms);
                            let sample = slot_sample_payload(&m.slug.0, &signal, m.beat);
                            if let Some(ref st) = store {
                                let _ = st.append_slot_sample(sample).await;
                            } else {
                                event_log.append(EventKind::SlotSample, sample);
                            }
                        }

                        if let Decision::Buy { side, px, .. } = &decision {
                            if fired_slugs.contains_key(&m.slug.0) {
                                continue;
                            }
                            if now_ms - last_fire_ms < cfg.strategy().cooldown_ms as i64 {
                                continue;
                            }
                            let signed = cache.lock().unwrap()
                                .get(&m.slug.0)
                                .and_then(|c| c.lookup_price(*side, *px).cloned());
                            let verdict = risk.check(&cfg.risk(), &m, &decision, signed.as_ref(), cfg.strategy().max_orders_per_market);
                            if let RiskVerdict::Reject(reason) = verdict {
                                event_log.append(EventKind::Decision, serde_json::json!({"reject": reason, "slug": m.slug.0}));
                                continue;
                            }
                            if let Some(signed) = signed {
                                let _ = exec_req_tx.send(ExecRequest {
                                    slug: m.slug.0.clone(),
                                    signed,
                                    order_type: static_cfg.execution.order_type.clone(),
                                    owner: secrets.address.clone(),
                                });
                                risk.mark_fired(&m.slug.0);
                                fired_slugs.insert(m.slug.0.clone(), ());
                                last_fire_ms = now_ms;
                            }
                        }
                    }
                }
            }
        }
    }
}

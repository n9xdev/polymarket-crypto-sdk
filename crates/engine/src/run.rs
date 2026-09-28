use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use poly_config::ConfigHandle;
use poly_domain::{Decision, Market};
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
use tokio::sync::mpsc;
use tracing::{info, warn};

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

        let (exec_req_tx, exec_req_rx) = mpsc::unbounded_channel();
        let (exec_log_tx, _exec_log_rx) = mpsc::unbounded_channel();
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
        let mut current_market: Option<Arc<Market>> = None;
        let bus = hub.bus.clone();
        let grid = static_cfg.grid_prices.clone();
        let exec_cfg = static_cfg.execution.clone();
        let dry_run = static_cfg.infra.dry_run;

        let mut fired_slug: Option<String> = None;
        let mut last_fire_ms: i64 = 0;

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
                            if current_market.is_none() {
                                current_market = Some(market.clone());
                            }
                        }
                        LifecycleEvent::MarketClosed { slug } => {
                            cache.lock().unwrap().remove(&slug.0);
                            if current_market.as_ref().is_some_and(|m| m.slug == slug) {
                                if let Some(m) = current_market.take() {
                                    let close_twap = bus.chainlink_twap_60.read().px;
                                    let official = fetch_outcome_up_won(&static_cfg.infra.gamma, &m.slug.0).await.ok().flatten();
                                    let report = build_report(
                                        &m,
                                        &m.slug.0,
                                        None,
                                        None,
                                        None,
                                        static_cfg.execution.size_usd,
                                        Some(close_twap),
                                        Some(bus.chainlink_spot.read().px),
                                        None,
                                        official,
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
                        let _ = st.write_heartbeat(
                            current_market.as_ref().map(|m| m.slug.0.as_str()),
                            slot_ages,
                            kill,
                            None,
                            dry_run,
                        ).await;
                    }
                }
                _ = interval.tick() => {
                    let Some(market) = current_market.clone() else { continue };
                    if market.beat.is_none() {
                        let twap = bus.chainlink_twap_60.read();
                        if twap.valid {
                            let mut m = (*market).clone();
                            if try_capture_beat(&mut m, twap.px, twap.src_ts_ms, Some(bus.chainlink_spot.read().px)).is_some() {
                                bus.beat.write(m.beat);
                                current_market = Some(Arc::new(m));
                            }
                        }
                    }
                    let m = current_market.as_ref().unwrap();
                    let open_ms = m.open_ts.timestamp_millis();
                    let close_ms = m.close_ts.timestamp_millis();
                    let signal = bus.snapshot(m.twap_lookback_sec, open_ms, close_ms);
                    let decision = strategy.on_signal(&signal, m);
                    event_log.append(
                        EventKind::Decision,
                        serde_json::json!({ "decision": format!("{:?}", decision) }),
                    );

                    if let Decision::Buy { side, px, .. } = &decision {
                        let now_ms = chrono::Utc::now().timestamp_millis();
                        if fired_slug.as_deref() == Some(m.slug.0.as_str()) {
                            continue;
                        }
                        if now_ms - last_fire_ms < cfg.strategy().cooldown_ms as i64 {
                            continue;
                        }
                        let signed = cache.lock().unwrap()
                            .get(&m.slug.0)
                            .and_then(|c| c.lookup_price(*side, *px).cloned());
                        let verdict = risk.check(&cfg.risk(), m, &decision, signed.as_ref(), cfg.strategy().max_orders_per_market);
                        if let RiskVerdict::Reject(reason) = verdict {
                            event_log.append(EventKind::Decision, serde_json::json!({"reject": reason}));
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
                            fired_slug = Some(m.slug.0.clone());
                            last_fire_ms = now_ms;
                        }
                    }
                }
            }
        }
    }
}

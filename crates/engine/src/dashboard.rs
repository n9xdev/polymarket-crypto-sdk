use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use poly_config::{ChainlinkSource, ConfigHandle};
use poly_domain::{Decision, Market, PriceSlot, Signal};
use poly_feeds::SignalBus;
use poly_market::compute_window_slugs;
use poly_risk::RiskGate;
use poly_signer::OrderCache;
use rust_decimal::Decimal;
use serde_json::{json, Map, Value};

pub struct DashboardCtx {
    pub started_at: chrono::DateTime<Utc>,
    pub last_decisions: Mutex<HashMap<String, Value>>,
    pub last_fill: Mutex<Option<Value>>,
    pub feed_reconnects: Mutex<u32>,
}

impl DashboardCtx {
    pub fn new() -> Self {
        Self {
            started_at: Utc::now(),
            last_decisions: Mutex::new(HashMap::new()),
            last_fill: Mutex::new(None),
            feed_reconnects: Mutex::new(0),
        }
    }

    pub fn record_fill(&self, fill: Value) {
        *self.last_fill.lock().unwrap() = Some(fill);
    }

    pub fn record_decision(&self, slug: &str, decision: &Decision) {
        let state = strategy_state(decision);
        self.last_decisions.lock().unwrap().insert(
            slug.to_string(),
            json!({
                "slug": slug,
                "decision": format!("{decision:?}"),
                "state": state,
                "ts": Utc::now().to_rfc3339(),
            }),
        );
    }
}

fn strategy_state(d: &Decision) -> &'static str {
    match d {
        Decision::Hold => "waiting",
        Decision::Buy { .. } => "eligible",
        Decision::Reject { .. } => "skipped",
    }
}

fn clob_market_ws_status(bus: &Arc<SignalBus>, now_ms: i64, has_active_market: bool) -> &'static str {
    if !has_active_market {
        return "idle (no market)";
    }
    if bus
        .clob_market_ws_connected
        .load(std::sync::atomic::Ordering::Acquire)
    {
        if bus.any_fresh_token_book(now_ms) {
            return "connected";
        }
        return "connected (awaiting book)";
    }
    if bus.any_fresh_token_book(now_ms) {
        return "connected (REST only)";
    }
    "disconnected"
}

fn optional_feed_ws_status(enabled: bool, slot: PriceSlot, now_ms: i64) -> String {
    if !enabled {
        return "off".into();
    }
    if slot.valid
        && slot.px > Decimal::ZERO
        && slot.src_ts_ms > 0
        && now_ms.saturating_sub(slot.src_ts_ms) <= 5000
    {
        "connected".into()
    } else {
        "disconnected".into()
    }
}

fn rtds_ws_status(static_cfg: &poly_config::Config, bus: &Arc<SignalBus>, now_ms: i64) -> String {
    match static_cfg.feeds.chainlink_source {
        ChainlinkSource::Rtds => {
            let twap = bus.chainlink_twap_30.read();
            if twap.valid && twap.src_ts_ms > 0 && now_ms.saturating_sub(twap.src_ts_ms) <= 5000 {
                "connected".into()
            } else {
                "disconnected".into()
            }
        }
        ChainlinkSource::DataStreams => "off (chainlink data_streams)".into(),
        ChainlinkSource::Polybolt => "off (polybolt)".into(),
    }
}

fn freshness_ms(age_ms: i64) -> &'static str {
    if age_ms < 0 || age_ms > 86400_000 {
        return "unknown";
    }
    if age_ms <= 2000 {
        "green"
    } else if age_ms <= 5000 {
        "amber"
    } else {
        "red"
    }
}

fn dec(v: Decimal) -> Value {
    Value::String(v.to_string())
}

fn dec_opt(v: Option<Decimal>) -> Value {
    v.map(dec).unwrap_or(Value::Null)
}

/// Omit missing/stale feed ticks so charts do not plot spurious zeros at startup.
fn dec_slot(slot: &poly_domain::PriceSlot) -> Value {
    if slot.valid && !slot.px.is_zero() {
        dec(slot.px)
    } else {
        Value::Null
    }
}

pub fn build_snapshot(
    cfg: &ConfigHandle,
    bus: &Arc<SignalBus>,
    active_markets: &HashMap<String, Arc<Market>>,
    cache: &Mutex<HashMap<String, OrderCache>>,
    risk: &RiskGate,
    ctx: &DashboardCtx,
    dry_run: bool,
    kill: bool,
    focus_tf: &str,
) -> Value {
    let static_cfg = cfg.config();
    let now_ms = Utc::now().timestamp_millis();
    let now_unix = Utc::now().timestamp();

    let mut windows = Map::new();
    for tf in &static_cfg.market.timeframes {
        let tf_str = tf.as_str();
        let markets_for_tf: Vec<&Arc<Market>> = active_markets
            .values()
            .filter(|m| m.timeframe.as_str() == tf_str)
            .collect();
        if markets_for_tf.is_empty() {
            windows.insert(
                tf.clone(),
                json!({ "active": false, "timeframe": tf, "markets": [] }),
            );
        } else {
            let market_views: Vec<Value> = markets_for_tf
                .iter()
                .map(|m| build_window_view(m, bus, &static_cfg, cache, ctx, risk, now_unix))
                .collect();
            windows.insert(
                tf.clone(),
                json!({
                    "active": true,
                    "timeframe": tf,
                    "markets": market_views,
                }),
            );
        }
    }

    let focus_market = static_cfg
        .market
        .assets
        .iter()
        .find_map(|asset| {
            active_markets.values().find(|m| {
                m.asset.as_str() == asset.as_str() && m.timeframe.as_str() == focus_tf
            })
        })
        .or_else(|| {
            active_markets
                .values()
                .find(|m| m.timeframe.as_str() == focus_tf)
        })
        .or_else(|| active_markets.values().find(|m| m.timeframe.as_str() == "5m"))
        .or_else(|| active_markets.values().next());

    let focus_asset = focus_market
        .map(|m| m.asset.as_str())
        .unwrap_or("btc");
    let spot_slot = bus.chainlink_spot_for(focus_asset);
    let twap_slot = bus.chainlink_twap60_for(focus_asset);
    let spot_age = feed_age(now_ms, spot_slot.src_ts_ms);
    let twap_age = feed_age(now_ms, twap_slot.src_ts_ms);
    let coinbase_slot = bus.coinbase_spot_for(focus_asset);
    let binance_slot = bus.binance_spot.read();
    let coinbase_age = feed_age(now_ms, coinbase_slot.src_ts_ms);
    let binance_age = feed_age(now_ms, binance_slot.src_ts_ms);

    let any_stale = active_markets.values().any(|m| {
        let open_ms = m.open_ts.timestamp_millis();
        let close_ms = m.close_ts.timestamp_millis();
        bus.snapshot(
            m.asset.as_str(),
            m.twap_lookback_sec,
            open_ms,
            close_ms,
            &m.token_up,
            &m.token_down,
        )
        .stale
        .any_required()
    });

    let degraded = kill || spot_age > 5000 || twap_age > 5000 || any_stale;
    let engine_state = if kill {
        "killed"
    } else if degraded {
        "degraded"
    } else {
        "running"
    };

    let hour_used: Decimal = risk.notional_hour.iter().map(|(_, v)| *v).sum();
    let day_used: Decimal = risk.notional_day.iter().map(|(_, v)| *v).sum();
    let orders_this_market = focus_market
        .map(|m| *risk.fired_per_slug.get(&m.slug.0).unwrap_or(&0))
        .unwrap_or(0);

    let (current_market, signal, strategy, next_market, last_decision) =
        if let Some(m) = focus_market {
            let w = build_window_view(m, bus, &static_cfg, cache, ctx, risk, now_unix);
            let next_slug = {
                let (_, next) = compute_window_slugs(m.asset, m.timeframe, now_unix);
                next.slug.0.clone()
            };
            (
                w.get("market").cloned(),
                w.get("signal").cloned(),
                w.get("strategy").cloned(),
                Some(json!({
                    "slug": next_slug,
                    "presign_ready": cache.lock().unwrap().contains_key(&next_slug),
                })),
                w.get("last_decision").cloned(),
            )
        } else {
            (None, None, None, None, None)
        };

    json!({
        "engine_state": engine_state,
        "dry_run": dry_run,
        "started_at": ctx.started_at.to_rfc3339(),
        "uptime_sec": (Utc::now() - ctx.started_at).num_seconds(),
        "region": static_cfg.infra.region,
        "focus_timeframe": focus_tf,
        "windows": Value::Object(windows),
        "feeds": {
            "chainlink_spot": { "age_ms": spot_age, "freshness": freshness_ms(spot_age), "px": dec(spot_slot.px), "valid": spot_slot.valid },
            "chainlink_twap": { "age_ms": twap_age, "freshness": freshness_ms(twap_age), "px": dec(twap_slot.px), "valid": twap_slot.valid },
            "coinbase_spot": { "age_ms": coinbase_age, "freshness": freshness_ms(coinbase_age), "px": dec(coinbase_slot.px), "valid": coinbase_slot.valid },
            "binance_spot": { "age_ms": binance_age, "freshness": freshness_ms(binance_age), "px": dec(binance_slot.px), "valid": binance_slot.valid },
            "ws": {
                "rtds": rtds_ws_status(&static_cfg, bus, now_ms),
                "clob_market": clob_market_ws_status(bus, now_ms, !active_markets.is_empty()),
                "clob_user": if dry_run { "n/a (dry-run)" } else { "unknown" },
                "coinbase": optional_feed_ws_status(static_cfg.feeds.coinbase.enabled, coinbase_slot, now_ms),
                "binance": optional_feed_ws_status(static_cfg.feeds.binance.enabled, binance_slot, now_ms),
            },
            "reconnect_count": *ctx.feed_reconnects.lock().unwrap(),
        },
        "current_market": current_market,
        "next_market": next_market,
        "signal": signal,
        "strategy": strategy,
        "last_decision": last_decision,
        "last_fill": ctx.last_fill.lock().unwrap().clone(),
        "wallet": { "usdc": risk.cached_usdc.to_string(), "open_exposure_notional": "0", "open_exposure_shares": "0" },
        "risk": {
            "hourly_used": hour_used.to_string(),
            "hourly_cap": static_cfg.risk.max_notional_per_hour.to_string(),
            "daily_used": day_used.to_string(),
            "daily_cap": static_cfg.risk.max_notional_per_day.to_string(),
            "orders_this_market": orders_this_market,
            "max_orders_per_market": static_cfg.strategy.max_orders_per_market,
        },
        "config": {
            "entry_after_sec": static_cfg.strategy.entry_after_sec,
            "best_ask_min": static_cfg.strategy.best_ask_min.to_string(),
            "twap_beat_diff_usd": static_cfg.strategy.twap_beat_diff_usd.to_string(),
            "size_usd": static_cfg.execution.size_usd.to_string(),
            "order_type": static_cfg.execution.order_type,
            "timeframes": static_cfg.market.timeframes,
            "assets": static_cfg.market.assets,
        },
        "alerts": build_alerts(spot_age, twap_age, kill, !active_markets.is_empty()),
    })
}

fn build_window_view(
    m: &Arc<Market>,
    bus: &Arc<SignalBus>,
    static_cfg: &poly_config::Config,
    cache: &Mutex<HashMap<String, OrderCache>>,
    ctx: &DashboardCtx,
    risk: &RiskGate,
    now_unix: i64,
) -> Value {
    let open_ms = m.open_ts.timestamp_millis();
    let close_ms = m.close_ts.timestamp_millis();
    let mut sig = bus.snapshot(
        m.asset.as_str(),
        m.twap_lookback_sec,
        open_ms,
        close_ms,
        &m.token_up,
        &m.token_down,
    );
    let beat = m.beat.or(sig.beat);
    sig.beat = beat;

    let diff = beat.and_then(|b| {
        if sig.chainlink_twap.valid {
            Some(sig.chainlink_twap.px - b)
        } else {
            None
        }
    });
    let side_hint = diff.map(|d| {
        if d > static_cfg.strategy.twap_beat_diff_usd {
            "up"
        } else if d < -static_cfg.strategy.twap_beat_diff_usd {
            "down"
        } else {
            "neutral"
        }
    });
    let bbo_side = side_hint.and_then(|s| match s {
        "up" => Some(sig.up_bbo),
        "down" => Some(sig.down_bbo),
        _ => None,
    });
    let best_ask_ok = bbo_side.map(|b| b.valid && b.ask > static_cfg.strategy.best_ask_min);

    let presign = cache.lock().unwrap().get(&m.slug.0).map(|c| {
        json!({
            "ready": c.len_signed(),
            "total": 198,
            "tick": m.tick.to_string(),
        })
    });

    let last_decision = ctx.last_decisions.lock().unwrap().get(&m.slug.0).cloned();

    let (_, next) = compute_window_slugs(m.asset, m.timeframe, now_unix);
    let next_slug = next.slug.0.clone();

    json!({
        "active": true,
        "timeframe": m.timeframe.as_str(),
        "market": {
            "slug": m.slug.0,
            "asset": m.asset.as_str(),
            "timeframe": m.timeframe.as_str(),
            "secs_left": sig.secs_left,
            "secs_into_window": sig.secs_into_window,
            "beat": dec_opt(beat),
            "open_ts": m.open_ts.to_rfc3339(),
            "close_ts": m.close_ts.to_rfc3339(),
            "token_up": m.token_up,
            "token_down": m.token_down,
            "presign": presign,
        },
        "next_market": {
            "slug": next_slug,
            "presign_ready": cache.lock().unwrap().contains_key(&next_slug),
        },
        "signal": signal_to_json(&sig),
        "strategy": {
            "state": last_decision.as_ref()
                .and_then(|d| d.get("state").cloned())
                .unwrap_or(Value::String("waiting".into())),
            "entry_after_sec": static_cfg.strategy.entry_after_sec,
            "secs_until_entry": (static_cfg.strategy.entry_after_sec as i64 - sig.secs_into_window).max(0),
            "twap_beat_diff_usd": static_cfg.strategy.twap_beat_diff_usd.to_string(),
            "twap_minus_beat": diff.map(dec),
            "side_hint": side_hint,
            "best_ask_min": static_cfg.strategy.best_ask_min.to_string(),
            "best_ask_ok": best_ask_ok,
        },
        "last_decision": last_decision,
        "orders_this_market": risk.fired_per_slug.get(&m.slug.0).copied().unwrap_or(0),
    })
}

fn feed_age(now_ms: i64, src_ts_ms: i64) -> i64 {
    if src_ts_ms == 0 {
        -1
    } else {
        now_ms.saturating_sub(src_ts_ms)
    }
}

fn build_alerts(spot_age: i64, twap_age: i64, kill: bool, has_market: bool) -> Value {
    let mut alerts = Vec::new();
    if kill {
        alerts.push(json!({ "level": "critical", "msg": "Kill switch active" }));
    }
    if spot_age > 5000 || spot_age < 0 {
        alerts.push(json!({ "level": "warn", "msg": "Chainlink spot stale or missing" }));
    }
    if twap_age > 5000 || twap_age < 0 {
        alerts.push(json!({ "level": "warn", "msg": "Chainlink TWAP stale or missing" }));
    }
    if has_market && spot_age < 0 {
        alerts.push(json!({ "level": "warn", "msg": "Beat snapshot may be missed" }));
    }
    Value::Array(alerts)
}

pub fn slot_sample_payload(slug: &str, signal: &Signal, beat: Option<Decimal>) -> Value {
    json!({
        "slug": slug,
        "beat": dec_opt(beat.or(signal.beat)),
        "chainlink_spot": dec_slot(&signal.chainlink_spot),
        "chainlink_twap": dec_slot(&signal.chainlink_twap),
        "coinbase_spot": dec_slot(&signal.coinbase_spot),
        "binance_spot": dec_slot(&signal.binance_spot),
        "coinbase_twap60": dec_slot(&signal.coinbase_twap60),
        "binance_twap60": dec_slot(&signal.binance_twap60),
        "coinbase_momentum_pct": dec_slot(&signal.coinbase_momentum_pct),
        "binance_momentum_pct": dec_slot(&signal.binance_momentum_pct),
        "up_ask": dec(signal.up_bbo.ask),
        "down_ask": dec(signal.down_bbo.ask),
        "stale": {
            "chainlink_spot": signal.stale.chainlink_spot,
            "chainlink_twap": signal.stale.chainlink_twap,
        },
    })
}

fn signal_to_json(sig: &Signal) -> Value {
    json!({
        "secs_left": sig.secs_left,
        "secs_into_window": sig.secs_into_window,
        "beat": dec_opt(sig.beat),
        "chainlink_spot": { "px": dec(sig.chainlink_spot.px), "stale": sig.stale.chainlink_spot },
        "chainlink_twap": { "px": dec(sig.chainlink_twap.px), "stale": sig.stale.chainlink_twap },
        "coinbase_spot": { "px": dec(sig.coinbase_spot.px), "stale": sig.stale.coinbase_spot },
        "binance_spot": { "px": dec(sig.binance_spot.px), "stale": sig.stale.binance_spot },
        "up_bbo": bbo_json(&sig.up_bbo),
        "down_bbo": bbo_json(&sig.down_bbo),
    })
}

fn bbo_json(b: &poly_domain::Bbo) -> Value {
    json!({
        "bid": dec(b.bid),
        "ask": dec(b.ask),
        "bid_sz": dec(b.bid_sz),
        "ask_sz": dec(b.ask_sz),
        "valid": b.valid,
        "spread": dec(b.ask - b.bid),
    })
}

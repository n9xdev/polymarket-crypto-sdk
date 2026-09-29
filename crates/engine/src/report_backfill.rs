use poly_domain::{Asset, Market, MarketStatus, Side, Slug, Timeframe};
use poly_settle::{build_report, fetch_outcome_up_won};
use poly_store::PostgresStore;
use rust_decimal::Decimal;
use tracing::{info, warn};

pub async fn backfill_paper_reports(store: &PostgresStore, gamma_base: &str, dry_run: bool) {
    backfill_scratch_reports(store, dry_run).await;
    backfill_missing_reports_from_fills(store, gamma_base, dry_run).await;
}

async fn backfill_scratch_reports(store: &PostgresStore, dry_run: bool) {
    let rows = match store.scratch_reports_with_fills().await {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "report backfill query failed");
            return;
        }
    };
    for row in rows {
        let side = parse_side(&row.side);
        let Some(side) = side else { continue };
        let market = market_from_row(
            &row.slug,
            &row.asset,
            &row.timeframe,
            row.beat,
            60,
            chrono::Utc::now(),
        );
        let report = build_report(
            &market,
            &row.slug,
            Some(side),
            Some(row.px),
            Some(row.px),
            row.size,
            row.close_twap,
            None,
            row.secs_into_window.map(i64::from),
            row.official_up_won,
            Some(row.fee),
            dry_run,
        );
        if let Err(e) = store.write_report(&report).await {
            warn!(slug = %row.slug, error = %e, "report backfill write failed");
        } else {
            info!(slug = %row.slug, result = ?report.result, pnl = %report.pnl, "backfilled market report from fill");
        }
    }
}

async fn backfill_missing_reports_from_fills(
    store: &PostgresStore,
    gamma_base: &str,
    dry_run: bool,
) {
    loop {
        let batch = match store.fills_missing_reports(100).await {
            Ok(b) => b,
            Err(e) => {
                warn!(error = %e, "fills_missing_reports query failed");
                return;
            }
        };
        if batch.is_empty() {
            break;
        }
        for row in batch {
            let Some(side) = parse_side(&row.side) else {
                continue;
            };
            let close_twap = store
                .last_slot_chainlink_twap(&row.slug)
                .await
                .ok()
                .flatten();
            let official = fetch_outcome_up_won(gamma_base, &row.slug)
                .await
                .ok()
                .flatten();
            let market = market_from_row(
                &row.slug,
                &row.asset,
                &row.timeframe,
                row.beat,
                row.twap_lookback_sec,
                row.close_ts,
            );
            let mut report = build_report(
                &market,
                &row.slug,
                Some(side),
                Some(row.px),
                Some(row.px),
                row.size,
                close_twap,
                None,
                None,
                official,
                Some(row.fee),
                dry_run,
            );
            report.closed_at = row.close_ts;
            if let Err(e) = store.write_report(&report).await {
                warn!(slug = %row.slug, error = %e, "missing report backfill write failed");
            } else {
                info!(
                    slug = %row.slug,
                    result = ?report.result,
                    pnl = %report.pnl,
                    "backfilled market report for filled slug"
                );
            }
        }
    }
}

fn market_from_row(
    slug: &str,
    asset: &str,
    timeframe: &str,
    beat: Option<Decimal>,
    twap_lookback_sec: i32,
    close_ts: chrono::DateTime<chrono::Utc>,
) -> Market {
    Market {
        slug: Slug(slug.to_string()),
        asset: Asset::parse(asset).unwrap_or(Asset::Btc),
        timeframe: Timeframe::parse(timeframe).unwrap_or(Timeframe::M5),
        condition_id: String::new(),
        token_up: String::new(),
        token_down: String::new(),
        tick: Decimal::new(1, 2),
        neg_risk: false,
        open_ts: close_ts - chrono::Duration::minutes(5),
        close_ts,
        twap_lookback_sec: twap_lookback_sec as u32,
        beat,
        spot_at_open: None,
        itode: false,
        status: MarketStatus::Closed,
        fee_rate: Some(Decimal::new(7, 2)),
    }
}

fn parse_side(s: &str) -> Option<Side> {
    match s {
        "Up" => Some(Side::Up),
        "Down" => Some(Side::Down),
        _ => None,
    }
}

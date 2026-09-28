use poly_domain::{Asset, Market, MarketStatus, Side, Slug, Timeframe};
use poly_settle::build_report;
use poly_store::PostgresStore;
use rust_decimal::Decimal;
use tracing::info;

pub async fn backfill_paper_reports(store: &PostgresStore, dry_run: bool) {
    let rows = match store.scratch_reports_with_fills().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "report backfill query failed");
            return;
        }
    };
    for row in rows {
        let side = parse_side(&row.side);
        let Some(side) = side else { continue };
        let market = Market {
            slug: Slug(row.slug.clone()),
            asset: Asset::parse(&row.asset).unwrap_or(Asset::Btc),
            timeframe: Timeframe::parse(&row.timeframe).unwrap_or(Timeframe::M5),
            condition_id: String::new(),
            token_up: String::new(),
            token_down: String::new(),
            tick: Decimal::new(1, 2),
            neg_risk: false,
            open_ts: chrono::Utc::now(),
            close_ts: chrono::Utc::now(),
            twap_lookback_sec: 60,
            beat: row.beat,
            spot_at_open: None,
            itode: false,
            status: MarketStatus::Closed,
            fee_rate: Some(Decimal::new(7, 2)),
        };
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
            tracing::warn!(slug = %row.slug, error = %e, "report backfill write failed");
        } else {
            info!(slug = %row.slug, result = ?report.result, pnl = %report.pnl, "backfilled market report from fill");
        }
    }
}

fn parse_side(s: &str) -> Option<Side> {
    match s {
        "Up" => Some(Side::Up),
        "Down" => Some(Side::Down),
        _ => None,
    }
}

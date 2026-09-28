use anyhow::Result;
use chrono::Utc;
use poly_domain::{Market, MarketReport, ReportResult, Side};
use rust_decimal::Decimal;

pub fn taker_fee_usdc(shares: Decimal, price: Decimal, fee_rate: Decimal) -> Decimal {
    shares * fee_rate * price * (Decimal::ONE - price)
}

pub fn model_up_wins(beat: Decimal, close_twap: Decimal) -> bool {
    close_twap > beat
}

pub async fn fetch_outcome_up_won(gamma_base: &str, slug: &str) -> Result<Option<bool>> {
    let url = format!("{}/markets/slug/{}", gamma_base.trim_end_matches('/'), slug);
    let resp = reqwest::get(&url).await?;
    let v: serde_json::Value = resp.json().await?;
    let prices_raw = v.get("outcomePrices").and_then(|x| x.as_str());
    let Some(prices_raw) = prices_raw else {
        return Ok(None);
    };
    let prices: Vec<String> = serde_json::from_str(prices_raw).unwrap_or_default();
    if prices.len() < 2 {
        return Ok(None);
    }
    let up_px: f64 = prices[0].parse().unwrap_or(0.0);
    Ok(Some(up_px >= 0.99))
}

pub fn resolve_result(
    side: Side,
    official_up_won: Option<bool>,
    model_up_wins: Option<bool>,
) -> ReportResult {
    if let Some(up_won) = official_up_won {
        return match side {
            Side::Up if up_won => ReportResult::Win,
            Side::Down if !up_won => ReportResult::Win,
            _ => ReportResult::Loss,
        };
    }
    if let Some(up_wins) = model_up_wins {
        return match (side, up_wins) {
            (Side::Up, true) | (Side::Down, false) => ReportResult::Win,
            _ => ReportResult::Loss,
        };
    }
    ReportResult::Scratch
}

pub fn build_report(
    market: &Market,
    slug: &str,
    side: Option<Side>,
    signed_px: Option<Decimal>,
    fill_vwap: Option<Decimal>,
    fill_size: Decimal,
    close_twap: Option<Decimal>,
    spot_at_fill: Option<Decimal>,
    secs_into: Option<i64>,
    official_up_won: Option<bool>,
    recorded_fee: Option<Decimal>,
    dry_run: bool,
) -> MarketReport {
    let beat = market.beat;
    let fee_rate = market.fee_rate.unwrap_or_else(|| Decimal::new(7, 2));
    let px = fill_vwap.or(signed_px).unwrap_or(Decimal::ZERO);
    let model = beat.zip(close_twap).map(|(b, c)| model_up_wins(b, c));

    let (result, fees, gross_pnl) = match side {
        None => (ReportResult::Scratch, Decimal::ZERO, Decimal::ZERO),
        Some(s) if fill_size.is_zero() || px.is_zero() => {
            (ReportResult::Scratch, Decimal::ZERO, Decimal::ZERO)
        }
        Some(s) => {
            let fees = recorded_fee
                .filter(|f| !f.is_zero())
                .unwrap_or_else(|| taker_fee_usdc(fill_size, px, fee_rate));
            let result = resolve_result(s, official_up_won, model);
            let gross = match result {
                ReportResult::Win => fill_size * (Decimal::ONE - px),
                ReportResult::Loss => -fill_size * px,
                ReportResult::Scratch => Decimal::ZERO,
            };
            (result, fees, gross)
        }
    };

    MarketReport {
        slug: slug.to_string(),
        result,
        pnl: gross_pnl,
        fees,
        fill_vwap,
        signed_px,
        beat,
        close_twap,
        chainlink_spot_at_fill: spot_at_fill,
        secs_into_window: secs_into,
        model_up_wins: model,
        official_up_won,
        side,
        dry_run,
        asset: market.asset.as_str().to_string(),
        timeframe: market.timeframe.as_str().to_string(),
        closed_at: Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poly_domain::{Asset, Market, MarketStatus, Slug, Timeframe};

    fn sample_market(beat: Decimal) -> Market {
        Market {
            slug: Slug("btc-updown-5m-1".into()),
            asset: Asset::Btc,
            timeframe: Timeframe::M5,
            condition_id: String::new(),
            token_up: String::new(),
            token_down: String::new(),
            open_ts: Utc::now(),
            close_ts: Utc::now(),
            beat: Some(beat),
            twap_lookback_sec: 60,
            tick: Decimal::new(1, 2),
            neg_risk: false,
            spot_at_open: None,
            itode: false,
            fee_rate: Some(Decimal::new(7, 2)),
            status: MarketStatus::Live,
        }
    }

    #[test]
    fn paper_settle_uses_model_when_official_missing() {
        let m = sample_market(Decimal::new(100, 0));
        let close = Decimal::new(101, 0);
        let r = build_report(
            &m,
            "slug",
            Some(Side::Up),
            Some(Decimal::new(62, 2)),
            None,
            Decimal::new(50, 0),
            Some(close),
            None,
            Some(240),
            None,
            Some(Decimal::ZERO),
            true,
        );
        assert_eq!(r.result, ReportResult::Win);
        assert!(r.pnl > Decimal::ZERO);
    }
}

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
) -> MarketReport {
    let beat = market.beat;
    let fee_rate = market.fee_rate.unwrap_or_else(|| Decimal::new(7, 2));
    let px = fill_vwap.or(signed_px).unwrap_or(Decimal::ZERO);
    let fees = taker_fee_usdc(fill_size, px, fee_rate);
    let model = beat.zip(close_twap).map(|(b, c)| model_up_wins(b, c));

    let result = match (side, official_up_won) {
        (Some(Side::Up), Some(true)) | (Some(Side::Down), Some(false)) => ReportResult::Win,
        (Some(_), Some(_)) => ReportResult::Loss,
        _ => ReportResult::Scratch,
    };
    let pnl = match result {
        ReportResult::Win => fill_size * (Decimal::ONE - px) - fees,
        ReportResult::Loss => -fill_size * px - fees,
        ReportResult::Scratch => Decimal::ZERO,
    };

    MarketReport {
        slug: slug.to_string(),
        result,
        pnl,
        fees,
        fill_vwap,
        signed_px,
        beat,
        close_twap,
        chainlink_spot_at_fill: spot_at_fill,
        secs_into_window: secs_into,
        model_up_wins: model,
        official_up_won,
        closed_at: Utc::now(),
    }
}

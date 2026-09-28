use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use poly_domain::{Asset, Market, MarketStatus, Side, Slug, Timeframe};
use rust_decimal::Decimal;
use serde::Deserialize;

#[derive(Clone)]
pub struct GammaClient {
    base: String,
    http: reqwest::Client,
}

impl GammaClient {
    pub fn new(base: String) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    pub async fn fetch_by_slug(&self, slug: &str) -> Result<GammaMarketRaw> {
        let url = format!("{}/markets/slug/{}", self.base, slug);
        let resp = self.http.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("gamma slug {} status {}", slug, resp.status()));
        }
        resp.json().await.context("parse gamma market")
    }

    pub async fn fetch_itode(&self, clob_base: &str, condition_id: &str) -> Result<bool> {
        let url = format!(
            "{}/clob-markets/{}",
            clob_base.trim_end_matches('/'),
            condition_id
        );
        let resp = self.http.get(&url).send().await?;
        if !resp.status().is_success() {
            return Ok(false);
        }
        let v: serde_json::Value = resp.json().await?;
        Ok(v.get("itode").and_then(|x| x.as_bool()).unwrap_or(false))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GammaMarketRaw {
    #[serde(rename = "conditionId", default)]
    pub condition_id: Option<String>,
    #[serde(rename = "clobTokenIds", default)]
    pub clob_token_ids: Option<serde_json::Value>,
    #[serde(default)]
    pub outcomes: Option<serde_json::Value>,
    #[serde(rename = "startDate", default)]
    pub start_date: Option<String>,
    #[serde(rename = "endDate", default)]
    pub end_date: Option<String>,
    /// Gamma sends this as a JSON number or string depending on market version.
    #[serde(rename = "orderPriceMinTickSize", default)]
    pub tick_size: Option<serde_json::Value>,
    #[serde(rename = "negRisk", default)]
    pub neg_risk: Option<bool>,
    #[serde(default)]
    pub fee: Option<serde_json::Value>,
    #[serde(rename = "cryptoMarketConfig", default)]
    pub crypto_market_config: Option<CryptoMarketConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CryptoMarketConfig {
    #[serde(rename = "twapLookbackSeconds", default)]
    pub twap_lookback_seconds: Option<u32>,
}

fn parse_decimal_value(v: &serde_json::Value) -> Option<Decimal> {
    match v {
        serde_json::Value::String(s) => s.parse().ok(),
        serde_json::Value::Number(n) => n.to_string().parse().ok(),
        _ => None,
    }
}

fn parse_string_list(v: &serde_json::Value) -> Vec<String> {
    if let Some(arr) = v.as_array() {
        return arr
            .iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect();
    }
    if let Some(s) = v.as_str() {
        if let Ok(parsed) = serde_json::from_str::<Vec<String>>(s) {
            return parsed;
        }
    }
    vec![]
}

pub fn gamma_to_market(
    raw: &GammaMarketRaw,
    slug: Slug,
    asset: Asset,
    tf: Timeframe,
    itode: bool,
) -> Result<Market> {
    let tokens = raw
        .clob_token_ids
        .as_ref()
        .map(parse_string_list)
        .unwrap_or_default();
    let outcomes = raw
        .outcomes
        .as_ref()
        .map(parse_string_list)
        .unwrap_or_default();
    let (token_up, token_down) = map_up_down(&tokens, &outcomes)?;
    let condition_id = raw
        .condition_id
        .clone()
        .ok_or_else(|| anyhow!("missing conditionId"))?;
    let tick = raw
        .tick_size
        .as_ref()
        .and_then(parse_decimal_value)
        .unwrap_or_else(|| Decimal::new(1, 2));
    let twap_lookback = raw
        .crypto_market_config
        .as_ref()
        .and_then(|c| c.twap_lookback_seconds)
        .unwrap_or(60);
    let open_ts = parse_dt(raw.start_date.as_deref()).unwrap_or_else(Utc::now);
    let close_ts = parse_dt(raw.end_date.as_deref()).unwrap_or_else(Utc::now);
    let fee_rate = raw.fee.as_ref().and_then(parse_decimal_value);

    Ok(Market {
        slug,
        asset,
        timeframe: tf,
        condition_id,
        token_up,
        token_down,
        tick,
        neg_risk: raw.neg_risk.unwrap_or(false),
        open_ts,
        close_ts,
        twap_lookback_sec: twap_lookback,
        beat: None,
        spot_at_open: None,
        itode,
        status: MarketStatus::Pending,
        fee_rate,
    })
}

fn map_up_down(tokens: &[String], outcomes: &[String]) -> Result<(String, String)> {
    if tokens.len() < 2 {
        return Err(anyhow!("need two clob token ids"));
    }
    if outcomes.len() >= 2 {
        let up_idx = outcomes
            .iter()
            .position(|o| {
                let l = o.to_lowercase();
                l == "up" || l == "yes"
            })
            .unwrap_or(0);
        let down_idx = outcomes
            .iter()
            .position(|o| {
                let l = o.to_lowercase();
                l == "down" || l == "no"
            })
            .unwrap_or(1);
        return Ok((tokens[up_idx].clone(), tokens[down_idx].clone()));
    }
    Ok((tokens[0].clone(), tokens[1].clone()))
}

fn parse_dt(s: Option<&str>) -> Option<DateTime<Utc>> {
    let s = s?;
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

pub fn token_for_side(market: &Market, side: Side) -> &str {
    match side {
        Side::Up => &market.token_up,
        Side::Down => &market.token_down,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_gamma_market_tick_as_number() {
        let raw: GammaMarketRaw = serde_json::from_str(
            r#"{
                "conditionId": "0xabc",
                "clobTokenIds": "[\"111\", \"222\"]",
                "outcomes": "[\"Up\", \"Down\"]",
                "orderPriceMinTickSize": 0.01
            }"#,
        )
        .expect("parse sample gamma json");
        assert_eq!(
            raw.tick_size.as_ref().and_then(parse_decimal_value),
            Some(Decimal::new(1, 2))
        );
    }
}

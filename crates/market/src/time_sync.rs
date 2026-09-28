use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct TimeResp {
    #[serde(default)]
    timestamp: Option<i64>,
}

/// Returns server_offset_sec such that polymarket_unix ≈ local_unix + offset.
pub async fn measure_server_offset(clob_base: &str) -> Result<f64> {
    let url = format!("{}/time", clob_base.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await?;
    let local_ms = chrono::Utc::now().timestamp_millis();
    let body: TimeResp = resp.json().await?;
    let server_ms = body.timestamp.unwrap_or(local_ms);
    Ok((server_ms - local_ms) as f64 / 1000.0)
}

pub fn adjusted_unix(now_unix: i64, server_offset_sec: f64) -> i64 {
    (now_unix as f64 + server_offset_sec).round() as i64
}

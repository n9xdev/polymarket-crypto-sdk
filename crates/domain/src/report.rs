use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::types::Side;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportResult {
    Win,
    Loss,
    Scratch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketReport {
    pub slug: String,
    pub result: ReportResult,
    pub pnl: Decimal,
    pub fees: Decimal,
    pub fill_vwap: Option<Decimal>,
    pub signed_px: Option<Decimal>,
    pub beat: Option<Decimal>,
    pub close_twap: Option<Decimal>,
    pub chainlink_spot_at_fill: Option<Decimal>,
    pub secs_into_window: Option<i64>,
    /// Dry-run comparison: Up wins if close TWAP strictly above beat.
    pub model_up_wins: Option<bool>,
    pub official_up_won: Option<bool>,
    pub side: Option<Side>,
    pub dry_run: bool,
    pub asset: String,
    pub timeframe: String,
    pub closed_at: DateTime<Utc>,
}

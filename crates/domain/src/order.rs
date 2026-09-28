use serde::{Deserialize, Serialize};

use crate::Side;

/// Presigned EIP-712 order blob + metadata for POST.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedOrder {
    pub side: Side,
    pub token_id: String,
    pub price_ticks: u32,
    /// JSON-serializable order body from the official SDK.
    pub order_json: serde_json::Value,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    pub order_id: String,
    pub slug: String,
    pub side: Side,
    pub px: rust_decimal::Decimal,
    pub size: rust_decimal::Decimal,
    pub fee: rust_decimal::Decimal,
    pub ts_ms: i64,
}

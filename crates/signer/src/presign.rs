use anyhow::{Context, Result};
use poly_config::ExecutionConfig;
use poly_domain::{Market, Side, SignedOrder, price_to_ticks};
use poly_market::token_for_side;
use rust_decimal::Decimal;
use tracing::info;

use crate::cache::OrderCache;

pub struct Presigner {
    pub private_key: Option<String>,
}

impl Presigner {
    pub fn new(private_key: Option<String>) -> Self {
        Self { private_key }
    }

    pub async fn presign_grid(
        &self,
        market: &Market,
        exec: &ExecutionConfig,
        grid: &[Decimal],
        dry_run: bool,
    ) -> Result<OrderCache> {
        let mut cache = OrderCache::default();
        if dry_run && self.private_key.is_none() {
            for px in grid {
                for side in [Side::Up, Side::Down] {
                    let token = token_for_side(market, side);
                    cache.insert(SignedOrder {
                        side,
                        token_id: token.to_string(),
                        price_ticks: price_to_ticks(*px),
                        order_json: serde_json::json!({
                            "tokenId": token,
                            "price": px.to_string(),
                            "side": "BUY",
                            "dryRun": true,
                        }),
                        signature: "dry_run".into(),
                    });
                }
            }
            info!(count = cache.len_signed(), "dry-run presign cache built");
            return Ok(cache);
        }

        let pk = self
            .private_key
            .as_ref()
            .context("POLY_PRIVATE_KEY required for live presign")?;

        for px in grid {
            for side in [Side::Up, Side::Down] {
                let signed = sign_limit_buy(pk, market, side, *px, exec).await?;
                cache.insert(signed);
            }
        }
        info!(count = cache.len_signed(), "presign cache built");
        Ok(cache)
    }
}

async fn sign_limit_buy(
    _pk: &str,
    market: &Market,
    side: Side,
    px: Decimal,
    exec: &ExecutionConfig,
) -> Result<SignedOrder> {
    let token = token_for_side(market, side);
    let size_shares = if px.is_zero() {
        Decimal::ZERO
    } else {
        (exec.size_usd / px).round_dp(2)
    };
    let _ = size_shares;
    // Placeholder: production uses polymarket_client_sdk_v2 order builder.
    Ok(SignedOrder {
        side,
        token_id: token.to_string(),
        price_ticks: price_to_ticks(px),
        order_json: serde_json::json!({
            "tokenId": token,
            "price": px.to_string(),
            "size": size_shares.to_string(),
            "side": "BUY",
            "negRisk": market.neg_risk,
            "tickSize": market.tick.to_string(),
        }),
        signature: "sdk_presign_placeholder".into(),
    })
}

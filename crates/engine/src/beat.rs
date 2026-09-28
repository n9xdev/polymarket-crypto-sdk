use poly_config::{ChainlinkSource, ConfigHandle};
use poly_domain::Market;
use poly_feeds::fetch_twap_at_timestamp;
use rust_decimal::Decimal;
use tracing::info;

use crate::secrets::EngineSecrets;

/// Polymarket "price to beat" = Chainlink Data Streams **TWAP 60s** at window open unix time.
pub async fn fetch_beat_at_window_open(
    cfg: &ConfigHandle,
    secrets: &EngineSecrets,
    market: &Market,
) -> Option<Decimal> {
    let c = cfg.config();
    if c.feeds.chainlink_source != ChainlinkSource::DataStreams {
        return None;
    }
    if secrets.chainlink_streams_user_id.is_empty() || secrets.chainlink_streams_secret.is_empty() {
        return None;
    }
    let feed_id = c
        .feeds
        .chainlink_data_streams
        .twap_60_feed_ids
        .get(market.asset.as_str())?;
    let open_unix = market.open_ts.timestamp();
    let px = fetch_twap_at_timestamp(
        &secrets.chainlink_streams_user_id,
        &secrets.chainlink_streams_secret,
        &c.feeds.chainlink_data_streams.rest_url,
        feed_id,
        open_unix,
    )
    .await?;
    let beat = Decimal::try_from(px).ok()?;
    info!(
        slug = %market.slug.0,
        open_unix,
        beat = %beat,
        "Chainlink TWAP60 @ window open (official beat)"
    );
    Some(beat)
}

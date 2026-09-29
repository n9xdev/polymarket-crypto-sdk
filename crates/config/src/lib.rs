mod raw;
mod validate;

use std::path::Path;
use std::sync::Arc;

use poly_domain::{Asset, Timeframe};

use rust_decimal::Decimal;
use thiserror::Error;
use tokio::sync::watch;

pub use raw::*;
pub use validate::grid_prices;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("validation: {0}")]
    Validation(String),
}

#[derive(Debug, Clone)]
pub struct Config {
    pub market: MarketConfig,
    pub feeds: FeedsConfig,
    pub execution: ExecutionConfig,
    pub strategy: StrategyConfig,
    pub risk: RiskConfig,
    pub infra: InfraConfig,
    pub grid_prices: Vec<Decimal>,
}

#[derive(Debug, Clone)]
pub struct ConfigHandle {
    pub static_cfg: Arc<Config>,
    strategy_tx: watch::Sender<StrategyConfig>,
    strategy_rx: watch::Receiver<StrategyConfig>,
    risk_tx: watch::Sender<RiskConfig>,
    risk_rx: watch::Receiver<RiskConfig>,
}

impl ConfigHandle {
    pub fn config(&self) -> Arc<Config> {
        self.static_cfg.clone()
    }

    pub fn strategy(&self) -> StrategyConfig {
        self.strategy_rx.borrow().clone()
    }

    pub fn risk(&self) -> RiskConfig {
        self.risk_rx.borrow().clone()
    }

    pub fn apply_strategy(&self, s: StrategyConfig) {
        let _ = self.strategy_tx.send(s);
    }

    pub fn apply_risk(&self, r: RiskConfig) {
        let _ = self.risk_tx.send(r);
    }

    pub fn subscribe_strategy(&self) -> watch::Receiver<StrategyConfig> {
        self.strategy_rx.clone()
    }

    pub fn subscribe_risk(&self) -> watch::Receiver<RiskConfig> {
        self.risk_rx.clone()
    }
}

pub fn load_from_path(path: &Path) -> Result<ConfigHandle, ConfigError> {
    let text = std::fs::read_to_string(path)?;
    load_from_str(&text)
}

pub fn load_from_str(text: &str) -> Result<ConfigHandle, ConfigError> {
    let raw: RawConfig = toml::from_str(text)?;
    let cfg = validate_and_build(raw)?;
    let (strategy_tx, strategy_rx) = watch::channel(cfg.strategy.clone());
    let (risk_tx, risk_rx) = watch::channel(cfg.risk.clone());
    Ok(ConfigHandle {
        static_cfg: Arc::new(cfg),
        strategy_tx,
        strategy_rx,
        risk_tx,
        risk_rx,
    })
}

fn validate_and_build(raw: RawConfig) -> Result<Config, ConfigError> {
    if raw.market.assets.is_empty() {
        return Err(ConfigError::Validation("market.assets empty".into()));
    }
    for a in &raw.market.assets {
        if Asset::parse(a).is_none() {
            return Err(ConfigError::Validation(format!("unknown asset: {a}")));
        }
    }
    if raw.market.timeframes.is_empty() {
        return Err(ConfigError::Validation("market.timeframes empty".into()));
    }
    for tf in &raw.market.timeframes {
        if Timeframe::parse(tf).is_none() {
            return Err(ConfigError::Validation(format!("unknown timeframe: {tf}")));
        }
    }

    if !raw.feeds.chainlink_spot {
        return Err(ConfigError::Validation(
            "chainlink_spot must be enabled".into(),
        ));
    }
    if !raw.feeds.chainlink_twap.enabled {
        return Err(ConfigError::Validation(
            "feeds.chainlink_twap must be enabled".into(),
        ));
    }

    let grid = grid_prices(
        raw.execution.grid_min,
        raw.execution.grid_max,
        raw.execution.grid_step,
    )?;
    if grid.len() != 99 {
        return Err(ConfigError::Validation(format!(
            "grid must have 99 levels, got {}",
            grid.len()
        )));
    }

    let source = raw::parse_chainlink_source(&raw.feeds.chainlink_source);
    if source == raw::ChainlinkSource::DataStreams {
        for a in &raw.market.assets {
            let key = a.to_lowercase();
            if !raw.feeds.chainlink_spot_feed_ids.contains_key(&key) {
                return Err(ConfigError::Validation(format!(
                    "data_streams: missing feeds.chainlink_spot_feed_ids.{key} for market.assets"
                )));
            }
            if !raw.feeds.chainlink_twap_60_feed_ids.contains_key(&key) {
                return Err(ConfigError::Validation(format!(
                    "data_streams: missing feeds.chainlink_twap_60_feed_ids.{key} for market.assets"
                )));
            }
        }
    }

    if raw.infra.decision_interval_ms < 10 || raw.infra.decision_interval_ms > 5000 {
        return Err(ConfigError::Validation(
            "infra.decision_interval_ms must be between 10 and 5000".into(),
        ));
    }

    Ok(Config {
        market: raw.market.into(),
        feeds: raw.feeds.into(),
        execution: raw.execution.into(),
        strategy: raw.strategy.into(),
        risk: raw.risk.into(),
        infra: raw.infra.into(),
        grid_prices: grid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_toml_loads() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../configs/default.toml");
        let h = load_from_path(&path).expect("load default");
        assert_eq!(h.static_cfg.grid_prices.len(), 99);
    }
}

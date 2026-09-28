use anyhow::{bail, Result};
use poly_config::{ChainlinkSource, InfraConfig};

#[derive(Debug, Clone)]
pub struct EngineSecrets {
    pub private_key: Option<String>,
    pub api_key: String,
    pub api_secret: String,
    pub passphrase: String,
    pub address: String,
    pub chainlink_streams_user_id: String,
    pub chainlink_streams_secret: String,
}

impl EngineSecrets {
    pub fn from_env(infra: &InfraConfig) -> Result<Self> {
        let private_key = std::env::var("POLY_PRIVATE_KEY").ok();
        let api_key = std::env::var("POLY_API_KEY").unwrap_or_default();
        let api_secret = std::env::var("POLY_API_SECRET").unwrap_or_default();
        let passphrase = std::env::var("POLY_PASSPHRASE").unwrap_or_default();
        let address = std::env::var("POLY_ADDRESS")
            .or_else(|_| std::env::var("POLY_FUNDER_ADDRESS"))
            .unwrap_or_default();
        let chainlink_streams_user_id = std::env::var("CHAINLINK_STREAMS_USER_ID")
            .or_else(|_| std::env::var("POLY_CHAINLINK_STREAMS_USER_ID"))
            .unwrap_or_default();
        let chainlink_streams_secret = std::env::var("CHAINLINK_STREAMS_SECRET")
            .or_else(|_| std::env::var("POLY_CHAINLINK_STREAMS_SECRET"))
            .unwrap_or_default();

        if !infra.dry_run {
            if private_key.is_none() {
                anyhow::bail!("POLY_PRIVATE_KEY required when dry_run=false");
            }
            if api_key.is_empty() || api_secret.is_empty() || passphrase.is_empty() {
                anyhow::bail!("POLY API credentials required when dry_run=false");
            }
            if address.is_empty() {
                bail!("POLY_ADDRESS required when dry_run=false");
            }
        }

        Ok(Self {
            private_key,
            api_key,
            api_secret,
            passphrase,
            address,
            chainlink_streams_user_id,
            chainlink_streams_secret,
        })
    }

    pub fn require_chainlink_streams(cfg: &poly_config::Config) -> Result<()> {
        if cfg.feeds.chainlink_source != ChainlinkSource::DataStreams {
            return Ok(());
        }
        let uid = std::env::var("CHAINLINK_STREAMS_USER_ID")
            .or_else(|_| std::env::var("POLY_CHAINLINK_STREAMS_USER_ID"))
            .unwrap_or_default();
        let secret = std::env::var("CHAINLINK_STREAMS_SECRET")
            .or_else(|_| std::env::var("POLY_CHAINLINK_STREAMS_SECRET"))
            .unwrap_or_default();
        if uid.is_empty() || secret.is_empty() {
            bail!(
                "CHAINLINK_STREAMS_USER_ID and CHAINLINK_STREAMS_SECRET required when feeds.chainlink_source = data_streams"
            );
        }
        Ok(())
    }

    pub fn ensure_dry_run_keys() -> Result<()> {
        Ok(())
    }
}

pub fn load_dotenv() {
    let _ = dotenvy::dotenv();
}

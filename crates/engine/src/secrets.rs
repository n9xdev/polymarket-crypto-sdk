use anyhow::Result;
use poly_config::InfraConfig;

#[derive(Debug, Clone)]
pub struct EngineSecrets {
    pub private_key: Option<String>,
    pub api_key: String,
    pub api_secret: String,
    pub passphrase: String,
    pub address: String,
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

        if !infra.dry_run {
            if private_key.is_none() {
                anyhow::bail!("POLY_PRIVATE_KEY required when dry_run=false");
            }
            if api_key.is_empty() || api_secret.is_empty() || passphrase.is_empty() {
                anyhow::bail!("POLY API credentials required when dry_run=false");
            }
            if address.is_empty() {
                anyhow::bail!("POLY_ADDRESS required when dry_run=false");
            }
        }

        Ok(Self {
            private_key,
            api_key,
            api_secret,
            passphrase,
            address,
        })
    }

    pub fn ensure_dry_run_keys() -> Result<()> {
        Ok(())
    }
}

pub fn load_dotenv() {
    let _ = dotenvy::dotenv();
}

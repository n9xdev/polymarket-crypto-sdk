use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info};

pub struct UserWsConfig {
    pub url: String,
    pub api_key: String,
    pub api_secret: String,
    pub passphrase: String,
    pub address: String,
}

pub async fn run_user_ws(cfg: UserWsConfig) -> Result<()> {
    loop {
        if let Err(e) = session(&cfg).await {
            error!(error = %e, "user ws ended");
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

async fn session(cfg: &UserWsConfig) -> Result<()> {
    let (ws, _) = connect_async(&cfg.url).await?;
    let (mut write, mut read) = ws.split();
    write
        .send(Message::Text(
            json!({
                "type": "user",
                "auth": {
                    "apiKey": cfg.api_key,
                    "secret": cfg.api_secret,
                    "passphrase": cfg.passphrase,
                },
                "markets": [],
            })
            .to_string()
            .into(),
        ))
        .await?;
    info!("user ws connected");
    while let Some(msg) = read.next().await {
        let _ = msg?;
    }
    Err(anyhow!("user ws disconnected"))
}

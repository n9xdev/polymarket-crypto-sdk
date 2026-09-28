use std::sync::Arc;

use anyhow::Result;
use poly_config::InfraConfig;
use poly_domain::SignedOrder;
use serde_json::json;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::l2::l2_signature;

#[derive(Debug, Clone)]
pub struct ExecRequest {
    pub slug: String,
    pub signed: SignedOrder,
    pub order_type: String,
    pub owner: String,
}

#[derive(Debug, Clone)]
pub struct ExecResponse {
    pub slug: String,
    pub success: bool,
    pub raw: serde_json::Value,
    pub dry_run: bool,
}

pub struct Executor {
    infra: InfraConfig,
    api_key: String,
    api_secret: String,
    passphrase: String,
    address: String,
    http: reqwest::Client,
    tx_log: Option<mpsc::UnboundedSender<ExecResponse>>,
}

impl Executor {
    pub fn new(
        infra: InfraConfig,
        api_key: String,
        api_secret: String,
        passphrase: String,
        address: String,
        tx_log: Option<mpsc::UnboundedSender<ExecResponse>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            infra,
            api_key,
            api_secret,
            passphrase,
            address,
            http: reqwest::Client::new(),
            tx_log,
        })
    }

    pub async fn warm_tls(&self) -> Result<()> {
        let url = format!("{}/time", self.infra.clob.trim_end_matches('/'));
        let _ = self.http.get(url).send().await?;
        Ok(())
    }

    pub async fn submit(&self, req: ExecRequest) -> Result<ExecResponse> {
        let body = json!({
            "order": req.signed.order_json,
            "owner": req.owner,
            "orderType": req.order_type,
            "deferExec": false,
        });
        let body_str = body.to_string();

        if self.infra.dry_run {
            info!(slug = %req.slug, body = %body_str, "dry-run POST /order");
            let resp = ExecResponse {
                slug: req.slug,
                success: true,
                raw: json!({"dryRun": true, "body": body}),
                dry_run: true,
            };
            if let Some(tx) = &self.tx_log {
                let _ = tx.send(resp.clone());
            }
            return Ok(resp);
        }

        let path = "/order";
        let ts = chrono::Utc::now().timestamp().to_string();
        let sig = l2_signature(&self.api_secret, &ts, "POST", path, &body_str);
        let url = format!("{}{}", self.infra.clob.trim_end_matches('/'), path);
        let http_resp = self
            .http
            .post(url)
            .header("POLY_ADDRESS", &self.address)
            .header("POLY_API_KEY", &self.api_key)
            .header("POLY_PASSPHRASE", &self.passphrase)
            .header("POLY_SIGNATURE", sig)
            .header("POLY_TIMESTAMP", ts)
            .header("Content-Type", "application/json")
            .body(body_str)
            .send()
            .await?;
        let status = http_resp.status();
        let raw: serde_json::Value = http_resp.json().await.unwrap_or(json!({}));
        let success = status.is_success();
        if !success {
            warn!(status = %status, raw = %raw, "order post failed");
        }
        let resp = ExecResponse {
            slug: req.slug,
            success,
            raw,
            dry_run: false,
        };
        if let Some(tx) = &self.tx_log {
            let _ = tx.send(resp.clone());
        }
        Ok(resp)
    }
}

pub fn spawn_executor_worker(
    executor: Arc<Executor>,
    mut rx: mpsc::UnboundedReceiver<ExecRequest>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(req) = rx.recv().await {
            if let Err(e) = executor.submit(req).await {
                warn!(error = %e, "executor submit error");
            }
        }
    })
}

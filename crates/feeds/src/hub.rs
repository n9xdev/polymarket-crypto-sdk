use std::sync::Arc;

use poly_config::{ChainlinkSource, ConfigHandle};
use poly_domain::Asset;
use tokio::task::JoinHandle;
use tracing::info;

use crate::bus::SignalBus;
use crate::clob_ws::{run_clob_books, ClobBookConfig};
use crate::polybolt::{run_polybolt, PolyBoltConfig};
use crate::rtds::{run_rtds_twap30, RtdsConfig};

pub struct FeedHub {
    pub bus: Arc<SignalBus>,
    handles: Vec<JoinHandle<()>>,
}

impl FeedHub {
    pub fn new() -> Self {
        Self {
            bus: SignalBus::new(),
            handles: Vec::new(),
        }
    }

    pub fn spawn_polybolt(
        &mut self,
        cfg: ConfigHandle,
        api_key: String,
        api_secret: String,
        passphrase: String,
    ) {
        let c = cfg.config();
        let assets: Vec<Asset> = c
            .market
            .assets
            .iter()
            .filter_map(|a| Asset::parse(a))
            .collect();
        let url = c.infra.polybolt.clone();
        let bus = self.bus.clone();
        if c.feeds.chainlink_source == ChainlinkSource::Polybolt {
            self.handles.push(tokio::spawn(async move {
                let _ = run_polybolt(
                    PolyBoltConfig {
                        url,
                        api_key,
                        api_secret,
                        passphrase,
                        assets,
                    },
                    bus,
                )
                .await;
            }));
        }
    }

    pub fn spawn_rtds_30(&mut self, cfg: ConfigHandle) {
        let c = cfg.config();
        if c.feeds.chainlink_source != ChainlinkSource::Rtds {
            return;
        }
        let assets: Vec<Asset> = c
            .market
            .assets
            .iter()
            .filter_map(|a| Asset::parse(a))
            .collect();
        let url = c.infra.rtds.clone();
        let bus = self.bus.clone();
        self.handles.push(tokio::spawn(async move {
            let _ = run_rtds_twap30(RtdsConfig { url, assets }, bus).await;
        }));
    }

    pub fn spawn_clob_books(&mut self, cfg: ConfigHandle, token_up: String, token_down: String) {
        let url = cfg.config().infra.clob_ws.clone();
        let bus = self.bus.clone();
        self.handles.push(tokio::spawn(async move {
            let _ = run_clob_books(
                ClobBookConfig {
                    url,
                    token_up,
                    token_down,
                },
                bus,
            )
            .await;
        }));
        info!("spawned clob book feeds");
    }
}

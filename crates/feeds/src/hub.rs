use std::sync::Arc;

use poly_config::{ChainlinkSource, ConfigHandle};
use poly_domain::Asset;
use tokio::task::JoinHandle;
use tracing::info;

use crate::bus::SignalBus;
use crate::clob_ws::{run_clob_books, ClobBookConfig};
use crate::data_streams::{run_data_streams, DataStreamsConfig, DataStreamsCredentials};
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

    pub fn spawn_data_streams(
        &mut self,
        cfg: ConfigHandle,
        user_id: String,
        secret: String,
    ) {
        let c = cfg.config();
        if c.feeds.chainlink_source != ChainlinkSource::DataStreams {
            return;
        }
        if user_id.is_empty() || secret.is_empty() {
            tracing::warn!(
                "chainlink_source=data_streams but CHAINLINK_STREAMS_USER_ID/SECRET unset"
            );
            return;
        }
        let enabled_assets = c.market.assets.clone();
        let streams = c.feeds.chainlink_data_streams.clone();
        let bus = self.bus.clone();
        self.handles.push(tokio::spawn(async move {
            let ds = DataStreamsConfig {
                streams,
                enabled_assets,
                creds: DataStreamsCredentials { user_id, secret },
            };
            let _ = run_data_streams(ds, bus).await;
        }));
        info!("spawned Chainlink Data Streams feeds");
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

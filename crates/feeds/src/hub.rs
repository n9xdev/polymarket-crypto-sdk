use std::sync::Arc;

use poly_config::{ChainlinkSource, ConfigHandle};
use poly_domain::Asset;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::info;

use crate::binance::{run_binance, BinanceConfig};
use crate::bus::SignalBus;
use crate::clob_book::{run_clob_book_manager, TokenSubscribe};
use crate::coinbase::{run_coinbase, CoinbaseConfig};
use crate::data_streams::{run_data_streams, DataStreamsConfig, DataStreamsCredentials};
use crate::polybolt::{run_polybolt, PolyBoltConfig};
use crate::rtds::{run_rtds_twap30, RtdsConfig};

pub struct FeedHub {
    pub bus: Arc<SignalBus>,
    handles: Vec<JoinHandle<()>>,
    clob_tx: Option<mpsc::UnboundedSender<TokenSubscribe>>,
}

impl FeedHub {
    pub fn new() -> Self {
        Self {
            bus: SignalBus::new(),
            handles: Vec::new(),
            clob_tx: None,
        }
    }

    fn ensure_clob(&mut self, cfg: &ConfigHandle) {
        if self.clob_tx.is_some() {
            return;
        }
        let ws_url = cfg.config().infra.clob_ws.clone();
        let rest = cfg.config().infra.clob.clone();
        let bus = self.bus.clone();
        let (tx, rx) = mpsc::unbounded_channel();
        self.clob_tx = Some(tx);
        self.handles.push(tokio::spawn(async move {
            run_clob_book_manager(ws_url, rest, bus, rx).await;
        }));
        info!("clob book manager started");
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
        self.ensure_clob(&cfg);
        if let Some(tx) = &self.clob_tx {
            let _ = tx.send(vec![token_up, token_down]);
        }
    }

    pub fn spawn_coinbase(&mut self, cfg: ConfigHandle) {
        let c = cfg.config();
        if !c.feeds.coinbase.enabled {
            return;
        }
        let assets: Vec<Asset> = c
            .market
            .assets
            .iter()
            .filter_map(|a| Asset::parse(a))
            .collect();
        let url = c.feeds.coinbase.ws_url.clone();
        let twap60 = c.feeds.coinbase.twap60;
        let bus = self.bus.clone();
        self.handles.push(tokio::spawn(async move {
            let _ = run_coinbase(CoinbaseConfig { url, assets, twap60 }, bus).await;
        }));
        info!("spawned Coinbase ticker feed");
    }

    pub fn spawn_binance(&mut self, cfg: ConfigHandle) {
        let c = cfg.config();
        if !c.feeds.binance.enabled {
            return;
        }
        let assets: Vec<Asset> = c
            .market
            .assets
            .iter()
            .filter_map(|a| Asset::parse(a))
            .collect();
        let ws_base = c.feeds.binance.ws_url.clone();
        let twap60 = c.feeds.binance.twap60;
        let bus = self.bus.clone();
        self.handles.push(tokio::spawn(async move {
            let _ = run_binance(
                BinanceConfig {
                    ws_base,
                    assets,
                    twap60,
                },
                bus,
            )
            .await;
        }));
        info!("spawned Binance ticker feed");
    }
}

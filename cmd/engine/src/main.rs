use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use poly_config::load_from_path;
use poly_engine::Engine;
use poly_strategy::{DefaultStrategy, Strategy};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "engine", about = "Polymarket crypto up/down trading engine")]
struct Args {
    #[arg(long, default_value = "configs/default.toml")]
    config: PathBuf,
}

fn init_rustls() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[tokio::main]
async fn main() -> Result<()> {
    init_rustls();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let args = Args::parse();
    let cfg = load_from_path(&args.config)?;
    let grid_step = cfg.config().execution.grid_step;
    let strategy_cfg = cfg.strategy();
    let strategy: Box<dyn Strategy> = Box::new(DefaultStrategy::new(strategy_cfg, grid_step));

    Engine::run(cfg, strategy).await
}

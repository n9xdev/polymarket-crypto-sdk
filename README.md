# polymarket-crypto-sdk

Modular monolith for Polymarket 5m/15m crypto up/down markets: one **engine** process (feeds, presign, strategy, risk, execution) and a read-only **dashboard**.

## Quick start

```bash
make help          # list targets
make engine        # trading engine (dry-run by default)
make dashboard     # read-only API on :8080
make web-dev       # Next.js UI on :3000
```

Or run binaries directly:

```bash
cargo run -p poly-crypto-engine -- --config configs/default.toml
cargo run -p poly-dashboard-api -- --config configs/default.toml
cd web && npm install && npm run dev
```

Copy [`.env.example`](.env.example) to `.env` for credentials.

**Chainlink prices:** default config uses paid [Data Streams](https://data.chain.link/streams) (`feeds.chainlink_source = "data_streams"`). Set `CHAINLINK_STREAMS_USER_ID` and `CHAINLINK_STREAMS_SECRET` in `.env`, and map feed IDs under `[feeds.chainlink_spot_feed_ids]` / `[feeds.chainlink_twap_60_feed_ids]` in config. For PolyBolt instead, set `chainlink_source = "polybolt"` and `POLY_API_*`.

Set `POLY_PRIVATE_KEY` and `POLY_ADDRESS` when `infra.dry_run = false`.

## Layout

See plan: `crates/*` modules, `cmd/engine`, `cmd/dashboard-api`, `web/`.

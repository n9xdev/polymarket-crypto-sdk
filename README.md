# polymarket-crypto-sdk

Modular monolith for Polymarket 5m/15m crypto up/down markets: one **engine** process (feeds, presign, strategy, risk, execution) and a read-only **dashboard**.

## Quick start

```bash
make help          # list targets
make db-up         # Postgres via Docker (poly/poly @ localhost:5433)
make engine        # trading engine (dry-run by default)
make dashboard     # starts DB if needed, then read-only API on :8081 (PORT=… to override)
make web-dev       # Next.js UI on :3000 (proxies /api to dashboard PORT)
```

**Database:** `make dashboard` runs `make db-up` first. Port **5433** is used so this stack does not clash with other Postgres on `:5432`. Stop with `make db-down`.

Default dashboard port is **8081** (see `Makefile`) to avoid clashing with other services on **8080**. `make web-dev` picks the first free port from **3000–3099** and prints the URL; set `WEB_PORT=3005` to pin a port.

Or run binaries directly:

```bash
cargo run -p poly-crypto-engine -- --config configs/default.toml
cargo run -p poly-dashboard-api -- --config configs/default.toml
cd web && npm install && npm run dev
```

Copy [`.env.example`](.env.example) to `.env` for credentials.

**Chainlink prices:** default config uses paid [Data Streams](https://data.chain.link/streams) (`feeds.chainlink_source = "data_streams"`). Set `CHAINLINK_STREAMS_USER_ID` and `CHAINLINK_STREAMS_SECRET` in `.env`, and map feed IDs under `[feeds.chainlink_spot_feed_ids]` / `[feeds.chainlink_twap_60_feed_ids]` in config. For PolyBolt instead, set `chainlink_source = "polybolt"` and `POLY_API_*`.

Set `POLY_PRIVATE_KEY` and `POLY_ADDRESS` when `infra.dry_run = false`.

## Dashboard

Run **engine**, **dashboard API**, and **web** together. The engine writes `engine_snapshot` (1 Hz) and `slot_sample` events (1 Hz) to Postgres for live charts.

UI sections: Overview, Live market, Price graph, Orders/fills, Market reports, Performance, Active hours, Daily volume, Price buckets, Risk (kill switch), System/ops (event tail). Global filters: asset, timeframe, result.

API routes include `/api/dashboard`, `/api/series`, `/api/analytics/*`, `/api/events`, `/api/reports/:slug`.

Not yet wired end-to-end (placeholders in UI): wallet balance, live fill ingestion from user WebSocket (paper mode simulates immediate fills in Postgres), user-WS status, weekday×hour heatmap, advanced stats (max drawdown, profit factor), chart markers for fills.

## Layout

See plan: `crates/*` modules, `cmd/engine`, `cmd/dashboard-api`, `web/`.

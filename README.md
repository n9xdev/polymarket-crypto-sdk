# polymarket-crypto-sdk

Modular monolith for Polymarket 5m/15m crypto up/down markets: one **engine** process (feeds, presign, strategy, risk, execution) and a read-only **dashboard**.

## Quick start

```bash
# Engine (dry-run by default)
cargo run -p poly-crypto-engine -- --config configs/default.toml

# Dashboard API
cargo run -p poly-dashboard-api -- --config configs/default.toml

# Web UI
cd web && npm install && npm run dev
```

Set `POLY_API_KEY`, `POLY_API_SECRET`, `POLY_PASSPHRASE` for PolyBolt feeds. Set `POLY_PRIVATE_KEY` and `POLY_ADDRESS` when `infra.dry_run = false`.

## Layout

See plan: `crates/*` modules, `cmd/engine`, `cmd/dashboard-api`, `web/`.

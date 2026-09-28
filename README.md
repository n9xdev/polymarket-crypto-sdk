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

Set `POLY_API_KEY`, `POLY_API_SECRET`, `POLY_PASSPHRASE` for PolyBolt feeds. Set `POLY_PRIVATE_KEY` and `POLY_ADDRESS` when `infra.dry_run = false`.

## Layout

See plan: `crates/*` modules, `cmd/engine`, `cmd/dashboard-api`, `web/`.

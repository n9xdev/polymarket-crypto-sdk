CREATE TABLE IF NOT EXISTS markets (
    slug TEXT PRIMARY KEY,
    asset TEXT NOT NULL,
    timeframe TEXT NOT NULL,
    condition_id TEXT NOT NULL,
    token_up TEXT NOT NULL,
    token_down TEXT NOT NULL,
    open_ts TIMESTAMPTZ NOT NULL,
    close_ts TIMESTAMPTZ NOT NULL,
    beat NUMERIC,
    twap_lookback_sec INT NOT NULL,
    tick NUMERIC NOT NULL,
    neg_risk BOOLEAN NOT NULL,
    status TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS orders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug TEXT NOT NULL REFERENCES markets(slug),
    side TEXT NOT NULL,
    signed_px NUMERIC NOT NULL,
    size NUMERIC NOT NULL,
    order_type TEXT NOT NULL,
    clob_order_id TEXT,
    status TEXT NOT NULL,
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    raw_resp JSONB
);

CREATE TABLE IF NOT EXISTS fills (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id UUID REFERENCES orders(id),
    slug TEXT NOT NULL,
    side TEXT NOT NULL,
    px NUMERIC NOT NULL,
    size NUMERIC NOT NULL,
    fee NUMERIC NOT NULL DEFAULT 0,
    ts TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS market_reports (
    slug TEXT PRIMARY KEY REFERENCES markets(slug),
    result TEXT NOT NULL,
    pnl NUMERIC NOT NULL,
    fees NUMERIC NOT NULL,
    fill_vwap NUMERIC,
    signed_px NUMERIC,
    beat NUMERIC,
    close_twap NUMERIC,
    chainlink_spot_at_fill NUMERIC,
    secs_into_window INT,
    model_up_wins BOOLEAN,
    official_up_won BOOLEAN,
    closed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS engine_heartbeats (
    ts TIMESTAMPTZ PRIMARY KEY DEFAULT NOW(),
    slug TEXT,
    slot_ages JSONB,
    kill BOOLEAN NOT NULL DEFAULT FALSE,
    last_error TEXT,
    dry_run BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS event_log (
    id BIGSERIAL PRIMARY KEY,
    ts TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    kind TEXT NOT NULL,
    payload JSONB NOT NULL
);

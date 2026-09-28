CREATE TABLE IF NOT EXISTS engine_snapshot (
    id INT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    payload JSONB NOT NULL DEFAULT '{}'::jsonb
);

INSERT INTO engine_snapshot (id, payload) VALUES (1, '{}'::jsonb)
ON CONFLICT (id) DO NOTHING;

ALTER TABLE market_reports ADD COLUMN IF NOT EXISTS dry_run BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE market_reports ADD COLUMN IF NOT EXISTS side TEXT;
ALTER TABLE market_reports ADD COLUMN IF NOT EXISTS asset TEXT;
ALTER TABLE market_reports ADD COLUMN IF NOT EXISTS timeframe TEXT;

CREATE INDEX IF NOT EXISTS idx_event_log_kind_ts ON event_log (kind, ts DESC);
CREATE INDEX IF NOT EXISTS idx_market_reports_closed ON market_reports (closed_at DESC);

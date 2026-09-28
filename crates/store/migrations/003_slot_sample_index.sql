CREATE INDEX IF NOT EXISTS idx_event_log_slot_sample_slug_ts
    ON event_log (kind, ((payload->'data'->>'slug')), ts DESC);

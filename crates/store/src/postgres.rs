use anyhow::Result;
use chrono::{DateTime, Utc};
use poly_domain::{MarketReport, ReportResult};
use rust_decimal::Decimal;
use sqlx::{PgPool, Row};
use serde_json::Value;

pub struct PostgresStore {
    pool: PgPool,
}

#[derive(Debug, Clone)]
pub struct FillMissingReport {
    pub slug: String,
    pub asset: String,
    pub timeframe: String,
    pub beat: Option<Decimal>,
    pub twap_lookback_sec: i32,
    pub close_ts: DateTime<Utc>,
    pub side: String,
    pub px: Decimal,
    pub size: Decimal,
    pub fee: Decimal,
}

#[derive(Debug, Clone)]
pub struct ScratchReportFill {
    pub slug: String,
    pub asset: String,
    pub timeframe: String,
    pub beat: Option<Decimal>,
    pub close_twap: Option<Decimal>,
    pub secs_into_window: Option<i32>,
    pub official_up_won: Option<bool>,
    pub side: String,
    pub px: Decimal,
    pub size: Decimal,
    pub fee: Decimal,
}

impl PostgresStore {
    pub async fn connect(url: &str) -> Result<Self> {
        let pool = PgPool::connect(url).await?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<()> {
        for file in ["001_init.sql", "002_dashboard.sql"] {
            let sql = match file {
                "001_init.sql" => include_str!("../migrations/001_init.sql"),
                "002_dashboard.sql" => include_str!("../migrations/002_dashboard.sql"),
                _ => continue,
            };
            for stmt in sql.split(';') {
                let s = stmt.trim();
                if !s.is_empty() {
                    sqlx::query(s).execute(&self.pool).await?;
                }
            }
        }
        Ok(())
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn insert_event(&self, kind: &str, payload: Value) -> Result<()> {
        sqlx::query("INSERT INTO event_log (kind, payload) VALUES ($1, $2)")
            .bind(kind)
            .bind(payload)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Persist chart sample without the decision event queue (1 Hz per market).
    pub async fn append_slot_sample(&self, data: Value) -> Result<()> {
        let payload = serde_json::json!({
            "ts": chrono::Utc::now().to_rfc3339(),
            "data": data,
        });
        self.insert_event("slot_sample", payload).await
    }

    pub async fn upsert_market(&self, slug: &str, asset: &str, tf: &str) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO markets (slug, asset, timeframe, condition_id, token_up, token_down,
                open_ts, close_ts, twap_lookback_sec, tick, neg_risk, status)
            VALUES ($1, $2, $3, '', '', '', NOW(), NOW(), 60, 0.01, false, 'pending')
            ON CONFLICT (slug) DO NOTHING
            "#,
        )
        .bind(slug)
        .bind(asset)
        .bind(tf)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_order(
        &self,
        slug: &str,
        side: &str,
        signed_px: Decimal,
        size: Decimal,
        order_type: &str,
        clob_order_id: Option<&str>,
        status: &str,
        raw_resp: Value,
    ) -> Result<uuid::Uuid> {
        let row = sqlx::query(
            r#"
            INSERT INTO orders (slug, side, signed_px, size, order_type, clob_order_id, status, raw_resp)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id
            "#,
        )
        .bind(slug)
        .bind(side)
        .bind(signed_px)
        .bind(size)
        .bind(order_type)
        .bind(clob_order_id)
        .bind(status)
        .bind(raw_resp)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("id"))
    }

    /// Filled slugs that have no row in `market_reports` (e.g. after DB truncate or engine downtime).
    pub async fn fills_missing_reports(&self, limit: i64) -> Result<Vec<FillMissingReport>> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT ON (f.slug)
                   f.slug,
                   m.asset,
                   m.timeframe,
                   m.beat,
                   m.twap_lookback_sec,
                   m.close_ts,
                   f.side,
                   f.px,
                   f.size,
                   f.fee
            FROM fills f
            INNER JOIN markets m ON m.slug = f.slug
            WHERE NOT EXISTS (SELECT 1 FROM market_reports mr WHERE mr.slug = f.slug)
            ORDER BY f.slug, f.ts ASC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| FillMissingReport {
                slug: r.get("slug"),
                asset: r.get("asset"),
                timeframe: r.get("timeframe"),
                beat: r.try_get("beat").ok(),
                twap_lookback_sec: r.get("twap_lookback_sec"),
                close_ts: r.get("close_ts"),
                side: r.get("side"),
                px: r.get("px"),
                size: r.get("size"),
                fee: r.get("fee"),
            })
            .collect())
    }

    pub async fn last_slot_chainlink_twap(&self, slug: &str) -> Result<Option<Decimal>> {
        let row = sqlx::query(
            r#"
            SELECT payload->'data'->>'chainlink_twap' AS twap
            FROM event_log
            WHERE kind = 'slot_sample'
              AND payload->'data'->>'slug' = $1
            ORDER BY ts DESC
            LIMIT 1
            "#,
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let s: Option<String> = row.try_get("twap").ok();
        Ok(s.and_then(|s| s.parse::<Decimal>().ok()))
    }

    pub async fn scratch_reports_with_fills(&self) -> Result<Vec<ScratchReportFill>> {
        let rows = sqlx::query(
            r#"
            SELECT mr.slug,
                   COALESCE(mr.asset, 'btc') AS asset,
                   COALESCE(mr.timeframe, '5m') AS timeframe,
                   mr.beat,
                   mr.close_twap,
                   mr.secs_into_window,
                   mr.official_up_won,
                   f.side,
                   f.px,
                   f.size,
                   f.fee
            FROM market_reports mr
            INNER JOIN fills f ON f.slug = mr.slug
            WHERE mr.result = 'scratch'
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| ScratchReportFill {
                slug: r.get("slug"),
                asset: r.get("asset"),
                timeframe: r.get("timeframe"),
                beat: r.try_get("beat").ok(),
                close_twap: r.try_get("close_twap").ok(),
                secs_into_window: r.try_get("secs_into_window").ok(),
                official_up_won: r.try_get("official_up_won").ok(),
                side: r.get("side"),
                px: r.get("px"),
                size: r.get("size"),
                fee: r.get("fee"),
            })
            .collect())
    }

    pub async fn primary_fill_for_slug(
        &self,
        slug: &str,
    ) -> Result<Option<(String, Decimal, Decimal, Decimal)>> {
        let row = sqlx::query(
            "SELECT side, px, size, fee FROM fills WHERE slug = $1 ORDER BY ts ASC LIMIT 1",
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| {
            (
                r.get("side"),
                r.get("px"),
                r.get("size"),
                r.get("fee"),
            )
        }))
    }

    pub async fn insert_fill(
        &self,
        order_id: uuid::Uuid,
        slug: &str,
        side: &str,
        px: Decimal,
        size: Decimal,
        fee: Decimal,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO fills (order_id, slug, side, px, size, fee)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(order_id)
        .bind(slug)
        .bind(side)
        .bind(px)
        .bind(size)
        .bind(fee)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn write_report(&self, report: &MarketReport) -> Result<()> {
        let result = match report.result {
            ReportResult::Win => "win",
            ReportResult::Loss => "loss",
            ReportResult::Scratch => "scratch",
        };
        sqlx::query(
            r#"
            INSERT INTO market_reports (slug, result, pnl, fees, fill_vwap, signed_px, beat,
                close_twap, chainlink_spot_at_fill, secs_into_window, model_up_wins, official_up_won,
                side, dry_run, asset, timeframe, closed_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
            ON CONFLICT (slug) DO UPDATE SET
                result = EXCLUDED.result,
                pnl = EXCLUDED.pnl,
                fees = EXCLUDED.fees,
                fill_vwap = EXCLUDED.fill_vwap,
                signed_px = EXCLUDED.signed_px,
                side = EXCLUDED.side,
                dry_run = EXCLUDED.dry_run,
                close_twap = EXCLUDED.close_twap,
                model_up_wins = EXCLUDED.model_up_wins,
                official_up_won = EXCLUDED.official_up_won,
                asset = EXCLUDED.asset,
                timeframe = EXCLUDED.timeframe
            "#,
        )
        .bind(&report.slug)
        .bind(result)
        .bind(report.pnl)
        .bind(report.fees)
        .bind(report.fill_vwap)
        .bind(report.signed_px)
        .bind(report.beat)
        .bind(report.close_twap)
        .bind(report.chainlink_spot_at_fill)
        .bind(report.secs_into_window)
        .bind(report.model_up_wins)
        .bind(report.official_up_won)
        .bind(report.side.map(|s| s.as_str()))
        .bind(report.dry_run)
        .bind(&report.asset)
        .bind(&report.timeframe)
        .bind(report.closed_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn write_heartbeat(
        &self,
        slug: Option<&str>,
        slot_ages: Value,
        kill: bool,
        last_error: Option<&str>,
        dry_run: bool,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO engine_heartbeats (slug, slot_ages, kill, last_error, dry_run)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(slug)
        .bind(slot_ages)
        .bind(kill)
        .bind(last_error)
        .bind(dry_run)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn total_pnl(&self) -> Result<Decimal> {
        let row = sqlx::query("SELECT COALESCE(SUM(pnl), 0) AS s FROM market_reports")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get::<Decimal, _>("s"))
    }

    pub async fn win_rate(&self) -> Result<f64> {
        let row = sqlx::query(
            r#"
            SELECT
              COALESCE(SUM(CASE WHEN result = 'win' THEN 1 ELSE 0 END), 0)::float8 AS wins,
              COUNT(*)::float8 AS total
            FROM market_reports
            "#,
        )
        .fetch_one(&self.pool)
        .await?;
        let wins: f64 = row.get("wins");
        let total: f64 = row.get("total");
        Ok(if total > 0.0 { wins / total } else { 0.0 })
    }

    pub async fn upsert_snapshot(&self, payload: Value) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO engine_snapshot (id, updated_at, payload)
            VALUES (1, NOW(), $1)
            ON CONFLICT (id) DO UPDATE SET updated_at = NOW(), payload = EXCLUDED.payload
            "#,
        )
        .bind(payload)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_snapshot(&self) -> Result<Option<Value>> {
        let row = sqlx::query("SELECT payload, updated_at FROM engine_snapshot WHERE id = 1")
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| {
            let mut v = r.get::<Value, _>("payload");
            if let Some(obj) = v.as_object_mut() {
                obj.insert(
                    "snapshot_updated_at".into(),
                    serde_json::json!(r.get::<DateTime<Utc>, _>("updated_at")),
                );
            }
            v
        }))
    }

    pub async fn list_events(&self, limit: i64, kind: Option<&str>) -> Result<Vec<Value>> {
        let rows = if let Some(k) = kind {
            sqlx::query(
                "SELECT ts, kind, payload FROM event_log WHERE kind = $1 ORDER BY ts DESC LIMIT $2",
            )
            .bind(k)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query("SELECT ts, kind, payload FROM event_log ORDER BY ts DESC LIMIT $1")
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
        };
        Ok(rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "ts": r.get::<DateTime<Utc>, _>("ts"),
                    "kind": r.get::<String, _>("kind"),
                    "payload": r.get::<Value, _>("payload"),
                })
            })
            .collect())
    }

    pub async fn slot_series(&self, slug: &str, limit: i64) -> Result<Vec<Value>> {
        let rows = sqlx::query(
            r#"
            SELECT ts, payload FROM event_log
            WHERE kind = 'slot_sample'
              AND payload->'data'->>'slug' = $1
            ORDER BY ts ASC
            LIMIT $2
            "#,
        )
        .bind(slug)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "ts": r.get::<DateTime<Utc>, _>("ts"),
                    "point": r.get::<Value, _>("payload").get("data").cloned().unwrap_or(Value::Null),
                })
            })
            .collect())
    }

    pub async fn list_reports_filtered(
        &self,
        limit: i64,
        asset: Option<&str>,
        timeframe: Option<&str>,
        result: Option<&str>,
    ) -> Result<Vec<Value>> {
        let rows = sqlx::query(
            r#"
            SELECT slug, COALESCE(asset,'') AS asset, COALESCE(timeframe,'') AS timeframe,
                   result, pnl, fees, fill_vwap, signed_px, beat, close_twap,
                   secs_into_window, side, dry_run, closed_at
            FROM market_reports
            WHERE ($2::text IS NULL OR lower(COALESCE(NULLIF(asset, ''), split_part(slug, '-', 1))) = lower($2))
              AND ($3::text IS NULL OR lower(COALESCE(NULLIF(timeframe, ''), split_part(slug, '-', 3))) = lower($3))
              AND ($4::text IS NULL OR result = $4)
            ORDER BY closed_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .bind(asset)
        .bind(timeframe)
        .bind(result)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| {
                let slug: String = r.get("slug");
                let asset_col: String = r.get("asset");
                let tf_col: String = r.get("timeframe");
                let asset_display = report_asset_from_slug(&slug, &asset_col);
                let tf_display = report_timeframe_from_slug(&slug, &tf_col);
                let beat: Option<Decimal> = r.try_get("beat").ok();
                let close_twap: Option<Decimal> = r.try_get("close_twap").ok();
                let twap_minus = match (close_twap, beat) {
                    (Some(t), Some(b)) => Some((t - b).to_string()),
                    _ => None,
                };
                serde_json::json!({
                    "slug": slug,
                    "asset": asset_display,
                    "timeframe": tf_display,
                    "result": r.get::<String, _>("result"),
                    "pnl": r.get::<Decimal, _>("pnl").to_string(),
                    "fees": r.get::<Decimal, _>("fees").to_string(),
                    "net": (r.get::<Decimal, _>("pnl") - r.get::<Decimal, _>("fees")).to_string(),
                    "fill_vwap": r.try_get::<Decimal, _>("fill_vwap").ok().map(|d| d.to_string()),
                    "signed_px": r.try_get::<Decimal, _>("signed_px").ok().map(|d| d.to_string()),
                    "beat": beat.map(|d| d.to_string()),
                    "close_twap": close_twap.map(|d| d.to_string()),
                    "twap_minus_beat": twap_minus,
                    "secs_into_window": r.try_get::<i32, _>("secs_into_window").ok(),
                    "side": r.try_get::<Option<String>, _>("side").ok().flatten(),
                    "dry_run": r.try_get::<bool, _>("dry_run").unwrap_or(true),
                    "closed_at": r.get::<DateTime<Utc>, _>("closed_at"),
                })
            })
            .collect())
    }

    pub async fn pnl_stats(&self) -> Result<Value> {
        let row = sqlx::query(
            r#"
            SELECT
              COALESCE(SUM(pnl), 0) AS total_pnl,
              COALESCE(SUM(fees), 0) AS total_fees,
              COUNT(*)::float8 AS trade_count,
              COALESCE(SUM(CASE WHEN result = 'win' THEN 1 ELSE 0 END), 0)::float8 AS wins,
              COALESCE(AVG(pnl), 0) AS avg_pnl,
              COALESCE(AVG(CASE WHEN result = 'win' THEN pnl END), 0) AS avg_winner,
              COALESCE(AVG(CASE WHEN result = 'loss' THEN pnl END), 0) AS avg_loser
            FROM market_reports
            "#,
        )
        .fetch_one(&self.pool)
        .await?;
        let wins: f64 = row.get("wins");
        let total: f64 = row.get("trade_count");
        Ok(serde_json::json!({
            "total_pnl": row.get::<Decimal, _>("total_pnl").to_string(),
            "total_fees": row.get::<Decimal, _>("total_fees").to_string(),
            "trade_count": total as i64,
            "win_rate": if total > 0.0 { wins / total } else { 0.0 },
            "avg_pnl_per_market": row.get::<Decimal, _>("avg_pnl").to_string(),
            "avg_winner": row.get::<Decimal, _>("avg_winner").to_string(),
            "avg_loser": row.get::<Decimal, _>("avg_loser").to_string(),
        }))
    }

    pub async fn analytics_by_hour(&self) -> Result<Value> {
        let rows = sqlx::query(
            r#"
            SELECT EXTRACT(HOUR FROM closed_at AT TIME ZONE 'UTC')::int AS hour,
                   COUNT(*)::int AS trades,
                   COALESCE(SUM(pnl), 0) AS pnl,
                   COALESCE(SUM(CASE WHEN result = 'win' THEN 1 ELSE 0 END), 0)::float8 AS wins
            FROM market_reports
            GROUP BY 1 ORDER BY 1
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        let hours: Vec<_> = rows
            .iter()
            .map(|r| {
                let trades: i32 = r.get("trades");
                let wins: f64 = r.get("wins");
                serde_json::json!({
                    "hour": r.get::<i32, _>("hour"),
                    "trades": trades,
                    "pnl": r.get::<Decimal, _>("pnl").to_string(),
                    "win_rate": if trades > 0 { wins / trades as f64 } else { 0.0 },
                })
            })
            .collect();
        Ok(Value::Array(hours))
    }

    pub async fn analytics_daily(&self, days: i64) -> Result<Value> {
        let rows = sqlx::query(
            r#"
            SELECT date_trunc('day', closed_at AT TIME ZONE 'UTC')::date AS day,
                   COUNT(*)::int AS fills,
                   COALESCE(SUM(pnl), 0) AS net_pnl,
                   COALESCE(SUM(fees), 0) AS fees
            FROM market_reports
            WHERE closed_at >= NOW() - make_interval(days => $1)
            GROUP BY 1 ORDER BY 1
            "#,
        )
        .bind(days)
        .fetch_all(&self.pool)
        .await?;
        Ok(Value::Array(
            rows.iter()
                .map(|r| {
                    serde_json::json!({
                        "day": r.get::<chrono::NaiveDate, _>("day").to_string(),
                        "fills": r.get::<i32, _>("fills"),
                        "net_pnl": r.get::<Decimal, _>("net_pnl").to_string(),
                        "fees": r.get::<Decimal, _>("fees").to_string(),
                    })
                })
                .collect(),
        ))
    }

    pub async fn analytics_buckets(&self) -> Result<Value> {
        let rows = sqlx::query(
            r#"
            SELECT
              CASE
                WHEN signed_px >= 0.95 THEN '0.95+'
                WHEN signed_px >= 0.90 THEN '0.90-0.94'
                WHEN signed_px >= 0.80 THEN '0.80-0.89'
                WHEN signed_px >= 0.50 THEN '0.50-0.79'
                ELSE '0.01-0.49'
              END AS bucket,
              COUNT(*)::int AS trades,
              COALESCE(SUM(pnl), 0) AS pnl,
              COALESCE(SUM(CASE WHEN result = 'win' THEN 1 ELSE 0 END), 0)::float8 AS wins
            FROM market_reports
            WHERE signed_px IS NOT NULL
            GROUP BY 1 ORDER BY 1
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(Value::Array(
            rows.iter()
                .map(|r| {
                    let trades: i32 = r.get("trades");
                    let wins: f64 = r.get("wins");
                    serde_json::json!({
                        "bucket": r.get::<String, _>("bucket"),
                        "trades": trades,
                        "pnl": r.get::<Decimal, _>("pnl").to_string(),
                        "win_rate": if trades > 0 { wins / trades as f64 } else { 0.0 },
                    })
                })
                .collect(),
        ))
    }

    pub async fn report_by_slug(&self, slug: &str) -> Result<Option<Value>> {
        let row = sqlx::query(
            "SELECT * FROM market_reports WHERE slug = $1",
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| {
            serde_json::json!({
                "slug": r.get::<String, _>("slug"),
                "result": r.get::<String, _>("result"),
                "pnl": r.get::<Decimal, _>("pnl").to_string(),
                "fees": r.get::<Decimal, _>("fees").to_string(),
                "beat": r.try_get::<Decimal, _>("beat").ok().map(|d| d.to_string()),
                "close_twap": r.try_get::<Decimal, _>("close_twap").ok().map(|d| d.to_string()),
                "signed_px": r.try_get::<Decimal, _>("signed_px").ok().map(|d| d.to_string()),
                "fill_vwap": r.try_get::<Decimal, _>("fill_vwap").ok().map(|d| d.to_string()),
                "closed_at": r.get::<DateTime<Utc>, _>("closed_at"),
            })
        }))
    }

    pub async fn latest_heartbeat(&self) -> Result<Option<Value>> {
        let row = sqlx::query(
            "SELECT slug, slot_ages, kill, last_error, dry_run, ts FROM engine_heartbeats ORDER BY ts DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| {
            serde_json::json!({
                "slug": r.get::<Option<String>, _>("slug"),
                "slot_ages": r.get::<Value, _>("slot_ages"),
                "kill": r.get::<bool, _>("kill"),
                "last_error": r.get::<Option<String>, _>("last_error"),
                "dry_run": r.get::<bool, _>("dry_run"),
                "ts": r.get::<DateTime<Utc>, _>("ts"),
            })
        }))
    }
}

/// Slugs are `{asset}-updown-{timeframe}-{epoch}`; older rows may have empty asset/timeframe columns.
fn report_asset_from_slug(slug: &str, asset_col: &str) -> String {
    if !asset_col.is_empty() {
        return asset_col.to_string();
    }
    slug.split('-').next().unwrap_or("").to_string()
}

fn report_timeframe_from_slug(slug: &str, timeframe_col: &str) -> String {
    if !timeframe_col.is_empty() {
        return timeframe_col.to_string();
    }
    let mut parts = slug.split('-');
    let _ = parts.next();
    let _ = parts.next();
    parts.next().unwrap_or("").to_string()
}

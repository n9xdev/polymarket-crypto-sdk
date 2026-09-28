use anyhow::Result;
use chrono::{DateTime, Utc};
use poly_domain::{MarketReport, ReportResult};
use rust_decimal::Decimal;
use sqlx::{PgPool, Row};
use serde_json::Value;

pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    pub async fn connect(url: &str) -> Result<Self> {
        let pool = PgPool::connect(url).await?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<()> {
        let sql = include_str!("../migrations/001_init.sql");
        for stmt in sql.split(';') {
            let s = stmt.trim();
            if !s.is_empty() {
                sqlx::query(s).execute(&self.pool).await?;
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

    pub async fn write_report(&self, report: &MarketReport) -> Result<()> {
        let result = match report.result {
            ReportResult::Win => "win",
            ReportResult::Loss => "loss",
            ReportResult::Scratch => "scratch",
        };
        sqlx::query(
            r#"
            INSERT INTO market_reports (slug, result, pnl, fees, fill_vwap, signed_px, beat,
                close_twap, chainlink_spot_at_fill, secs_into_window, model_up_wins, official_up_won, closed_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
            ON CONFLICT (slug) DO UPDATE SET
                result = EXCLUDED.result,
                pnl = EXCLUDED.pnl,
                fees = EXCLUDED.fees
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

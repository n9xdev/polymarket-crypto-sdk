"use client";

import { useMemo } from "react";
import clsx from "clsx";
import {
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import type { DashboardPayload } from "../lib/api";
import {
  bestWorstTrades,
  computeRiskMetrics,
  formatUsd,
  pnlClass,
  winLossCounts,
  type PnlSummary,
} from "../lib/analytics";

type Props = {
  pnl?: PnlSummary;
  reports: Record<string, unknown>[];
  hours: Record<string, unknown>[];
  daily: Record<string, unknown>[];
  buckets: Record<string, unknown>[];
};

export default function AnalyticsHub({ pnl, reports, hours, daily, buckets }: Props) {
  const { wins, losses } = winLossCounts(reports);
  const decided = wins + losses;
  const winRate = decided > 0 ? wins / decided : Number(pnl?.win_rate ?? 0);
  const { best, worst } = bestWorstTrades(reports);
  const risk = useMemo(() => computeRiskMetrics(reports), [reports]);

  const cumulative = useMemo(() => {
    let sum = 0;
    return [...reports].reverse().map((r, i) => {
      sum += parseFloat(String(r.net ?? r.pnl ?? 0));
      return { i, pnl: sum };
    });
  }, [reports]);

  const totalPnl = parseFloat(String(pnl?.total_pnl ?? 0));
  const totalFees = parseFloat(String(pnl?.total_fees ?? 0));

  const hourCells = useMemo(() => {
    const byHour = new Map<number, number>();
    for (const h of hours) {
      byHour.set(Number(h.hour ?? 0), Number(h.trades ?? 0));
    }
    const max = Math.max(1, ...Array.from(byHour.values()));
    return Array.from({ length: 24 }, (_, hour) => ({
      hour,
      trades: byHour.get(hour) ?? 0,
      intensity: (byHour.get(hour) ?? 0) / max,
    }));
  }, [hours]);

  const dailyData = daily.map((d) => {
    const net = parseFloat(String(d.net_pnl ?? 0));
    const fills = Number(d.fills ?? 0);
    return { day: String(d.day).slice(5), net, fills, positive: net >= 0 };
  });

  const bucketRows = buckets.map((b) => ({
    bucket: String(b.bucket ?? ""),
    trades: Number(b.trades ?? 0),
    pnl: parseFloat(String(b.pnl ?? 0)),
    wins: Number(b.wins ?? 0),
  }));

  return (
    <div className="analytics-page">
      <section className="card card-hero">
        <SectionTitle dot="green" title="Performance" subtitle="Closed market reports · paper / dry-run" />
        <div className="perf-hero">
          <div className="win-rate-block">
            <div
              className="win-ring"
              style={{
                background: `conic-gradient(var(--green) ${winRate * 360}deg, var(--ring-track) 0)`,
              }}
            />
            <div>
              <div className="win-rate-label">Win rate</div>
              <div className="win-rate-value">{(winRate * 100).toFixed(1)}%</div>
            </div>
          </div>
          <div className="win-bar-wrap">
            <div className="win-bar">
              <div className="win-bar-wins" style={{ width: `${decided ? (wins / decided) * 100 : 50}%` }} />
            </div>
            <div className="win-legend">
              <span><i className="dot green" /> {wins} wins</span>
              <span><i className="dot red" /> {losses} losses</span>
            </div>
          </div>
        </div>

        <div className="stat-grid two-col">
          <Stat label="Realized PnL" value={formatUsd(totalPnl)} className={pnlClass(totalPnl)} />
          <Stat label="Fees" value={formatUsd(-Math.abs(totalFees))} className="stat-negative" />
          <Stat label="Markets" value={String(pnl?.trade_count ?? reports.length)} />
          <Stat label="Avg / market" value={String(pnl?.avg_pnl_per_market ?? "—")} />
          <Stat label="Avg winner" value={String(pnl?.avg_winner ?? "—")} className="stat-positive" />
          <Stat label="Avg loser" value={String(pnl?.avg_loser ?? "—")} className="stat-negative" />
        </div>

        {(best || worst) && (
          <div className="top-trades">
            <h4 className="subsection-title">Top trades</h4>
            <div className="grid-2">
              {best && (
                <TradeCard title="Best" slug={best.slug} net={best.net} side={best.side} px={best.signed_px} />
              )}
              {worst && (
                <TradeCard title="Worst" slug={worst.slug} net={worst.net} side={worst.side} px={worst.signed_px} />
              )}
            </div>
          </div>
        )}

        {cumulative.length > 1 && (
          <div className="chart-wrap chart-compact" style={{ marginTop: 20 }}>
            <div className="subsection-title">Cumulative PnL</div>
            <ResponsiveContainer width="100%" height="100%">
              <LineChart data={cumulative}>
                <CartesianGrid stroke="var(--panel-border)" strokeDasharray="3 3" />
                <XAxis dataKey="i" hide />
                <YAxis stroke="var(--muted)" fontSize={11} tickFormatter={(v) => `$${v}`} />
                <Tooltip
                  contentStyle={{ background: "var(--panel)", border: "1px solid var(--panel-border)" }}
                  formatter={(v) => [formatUsd(Number(v)), "PnL"]}
                />
                <Line type="monotone" dataKey="pnl" stroke="var(--green)" dot={false} strokeWidth={2} />
              </LineChart>
            </ResponsiveContainer>
          </div>
        )}
      </section>

      <div className="grid-2">
        <section className="card">
          <SectionTitle dot="cyan" title="Active hours" subtitle="UTC · trade count by hour" />
          <div className="hour-heatmap">
            {hourCells.map((c) => (
              <div
                key={c.hour}
                className="hour-cell"
                title={`${c.hour}:00 UTC · ${c.trades} trades`}
                style={{ opacity: 0.15 + c.intensity * 0.85 }}
              />
            ))}
          </div>
          <div className="hour-labels">
            <span>0</span>
            <span>12</span>
            <span>24</span>
          </div>
        </section>

        <section className="card">
          <SectionTitle dot="blue" title="Daily activity" subtitle="Net PnL & fill count (30d)" />
          {dailyData.length === 0 ? (
            <p className="empty">No daily data yet.</p>
          ) : (
            <div className="chart-wrap chart-compact">
              <ResponsiveContainer width="100%" height="100%">
                <BarChart data={dailyData}>
                  <CartesianGrid stroke="var(--panel-border)" strokeDasharray="3 3" />
                  <XAxis dataKey="day" stroke="var(--muted)" fontSize={10} />
                  <YAxis stroke="var(--muted)" fontSize={11} />
                  <Tooltip contentStyle={{ background: "var(--panel)", border: "1px solid var(--panel-border)" }} />
                  <Bar dataKey="net" name="Net PnL">
                    {dailyData.map((d, i) => (
                      <Cell key={i} fill={d.positive ? "var(--green)" : "var(--red)"} />
                    ))}
                  </Bar>
                </BarChart>
              </ResponsiveContainer>
            </div>
          )}
        </section>
      </div>

      <section className="card">
        <SectionTitle dot="red" title="Risk scores" subtitle="Derived from closed-market PnL series" />
        <div className="risk-score-grid">
          <RiskCard label="Max drawdown" value={formatUsd(risk.maxDrawdown, true)} hint="Peak-to-trough on cumulative PnL" />
          <RiskCard label="Sharpe (approx)" value={risk.sharpe != null ? risk.sharpe.toFixed(2) : "—"} hint="Per-market returns" />
          <RiskCard label="Sortino (approx)" value={risk.sortino != null ? risk.sortino.toFixed(2) : "—"} hint="Downside vol only" />
          <RiskCard label="Calmar (approx)" value={risk.calmar != null ? risk.calmar.toFixed(2) : "—"} hint="Return / |max DD|" />
        </div>
      </section>

      <section className="card">
        <SectionTitle dot="amber" title="Price bucket breakdown" subtitle="By average entry (signed_px)" />
        {bucketRows.length === 0 ? (
          <p className="empty">No bucket data — need fills with signed_px on reports.</p>
        ) : (
          <div className="grid-3 bucket-charts">
            <BucketChart title="Trades" dataKey="trades" data={bucketRows} color="var(--amber)" />
            <BucketChart
              title="PnL"
              dataKey="pnl"
              data={bucketRows}
              colorFn={(v) => (v >= 0 ? "var(--green)" : "var(--red)")}
            />
            <BucketChart title="Wins" dataKey="wins" data={bucketRows} color="var(--accent)" />
          </div>
        )}
      </section>
    </div>
  );
}

function SectionTitle({
  title,
  subtitle,
  dot,
}: {
  title: string;
  subtitle: string;
  dot: "green" | "cyan" | "blue" | "red" | "amber";
}) {
  return (
    <header className="section-head">
      <span className={clsx("section-dot", dot)} />
      <div>
        <h2 className="section-title">{title}</h2>
        <p className="section-sub">{subtitle}</p>
      </div>
    </header>
  );
}

function Stat({ label, value, className }: { label: string; value: string; className?: string }) {
  return (
    <div className="stat-item">
      <div className="stat-label">{label}</div>
      <div className={clsx("stat-value", className)}>{value}</div>
    </div>
  );
}

function TradeCard({
  title,
  slug,
  net,
  side,
  px,
}: {
  title: string;
  slug: string;
  net: number;
  side: string;
  px: string;
}) {
  return (
    <div className="trade-card">
      <div className="trade-card-label">{title}</div>
      <div className="trade-card-slug">{slug}</div>
      <div className="trade-card-meta">{side} @ {px}</div>
      <div className={clsx("trade-card-pnl", pnlClass(net))}>{formatUsd(net)}</div>
    </div>
  );
}

function RiskCard({ label, value, hint }: { label: string; value: string; hint: string }) {
  return (
    <div className="risk-card">
      <div className="risk-card-label">{label}</div>
      <div className="risk-card-value">{value}</div>
      <div className="risk-card-hint">{hint}</div>
    </div>
  );
}

function BucketChart({
  title,
  dataKey,
  data,
  color,
  colorFn,
}: {
  title: string;
  dataKey: "trades" | "pnl" | "wins";
  data: { bucket: string; trades: number; pnl: number; wins: number }[];
  color?: string;
  colorFn?: (v: number) => string;
}) {
  return (
    <div>
      <div className="subsection-title">{title}</div>
      <div className="chart-wrap chart-compact">
        <ResponsiveContainer width="100%" height="100%">
          <BarChart data={data}>
            <CartesianGrid stroke="var(--panel-border)" strokeDasharray="3 3" />
            <XAxis dataKey="bucket" stroke="var(--muted)" fontSize={9} angle={-25} textAnchor="end" height={50} />
            <YAxis stroke="var(--muted)" fontSize={10} />
            <Tooltip contentStyle={{ background: "var(--panel)", border: "1px solid var(--panel-border)" }} />
            <Bar dataKey={dataKey}>
              {data.map((row, i) => {
                const v = row[dataKey];
                const fill = colorFn ? colorFn(v) : color ?? "var(--blue)";
                return <Cell key={i} fill={fill} />;
              })}
            </Bar>
          </BarChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}

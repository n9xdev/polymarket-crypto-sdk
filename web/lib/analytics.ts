import type { DashboardPayload } from "./api";

export type RiskMetrics = {
  maxDrawdown: number;
  sharpe: number | null;
  sortino: number | null;
  calmar: number | null;
};

export function computeRiskMetrics(reports: Record<string, unknown>[]): RiskMetrics {
  const nets = [...reports]
    .reverse()
    .map((r) => parseFloat(String(r.net ?? r.pnl ?? 0)))
    .filter((n) => Number.isFinite(n));

  if (nets.length === 0) {
    return { maxDrawdown: 0, sharpe: null, sortino: null, calmar: null };
  }

  let peak = 0;
  let cum = 0;
  let maxDd = 0;
  for (const x of nets) {
    cum += x;
    peak = Math.max(peak, cum);
    maxDd = Math.min(maxDd, cum - peak);
  }

  const mean = nets.reduce((a, b) => a + b, 0) / nets.length;
  const variance =
    nets.reduce((s, x) => s + (x - mean) ** 2, 0) / Math.max(1, nets.length - 1);
  const std = Math.sqrt(variance);
  const downside = nets.filter((x) => x < 0);
  const downVar =
    downside.length > 0
      ? downside.reduce((s, x) => s + x ** 2, 0) / downside.length
      : 0;
  const downStd = Math.sqrt(downVar);

  const sharpe = std > 1e-9 ? (mean / std) * Math.sqrt(nets.length) : null;
  const sortino = downStd > 1e-9 ? (mean / downStd) * Math.sqrt(nets.length) : null;
  const totalReturn = nets.reduce((a, b) => a + b, 0);
  const calmar =
    maxDd < -1e-9 ? totalReturn / Math.abs(maxDd) : totalReturn > 0 ? null : null;

  return { maxDrawdown: maxDd, sharpe, sortino, calmar };
}

export function winLossCounts(reports: Record<string, unknown>[]) {
  let wins = 0;
  let losses = 0;
  let scratch = 0;
  for (const r of reports) {
    const res = String(r.result ?? "");
    if (res === "win") wins += 1;
    else if (res === "loss") losses += 1;
    else scratch += 1;
  }
  return { wins, losses, scratch };
}

export function bestWorstTrades(reports: Record<string, unknown>[]) {
  const withPnl = reports
    .map((r) => ({
      slug: String(r.slug ?? ""),
      net: parseFloat(String(r.net ?? r.pnl ?? 0)),
      side: String(r.side ?? "—"),
      signed_px: String(r.signed_px ?? "—"),
    }))
    .filter((r) => Number.isFinite(r.net) && r.slug);
  if (!withPnl.length) return { best: null, worst: null };
  const best = withPnl.reduce((a, b) => (b.net > a.net ? b : a));
  const worst = withPnl.reduce((a, b) => (b.net < a.net ? b : a));
  return { best, worst };
}

export function formatUsd(n: number, compact = false) {
  if (!Number.isFinite(n)) return "—";
  const sign = n >= 0 ? "+" : "";
  if (compact && Math.abs(n) >= 1000) {
    return `${sign}$${(n / 1000).toFixed(1)}K`;
  }
  return `${sign}$${n.toFixed(2)}`;
}

export function pnlClass(n: number) {
  if (n > 0) return "stat-positive";
  if (n < 0) return "stat-negative";
  return "";
}

export type PnlSummary = DashboardPayload["pnl"];

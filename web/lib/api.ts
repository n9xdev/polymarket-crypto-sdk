export type DashboardPayload = {
  snapshot?: Record<string, unknown> | null;
  heartbeat?: Record<string, unknown> | null;
  pnl?: {
    total_pnl?: string;
    total_fees?: string;
    trade_count?: number;
    win_rate?: number;
    avg_pnl_per_market?: string;
    avg_winner?: string;
    avg_loser?: string;
  };
  config_meta?: Record<string, unknown>;
};

export async function fetchDashboard(): Promise<DashboardPayload> {
  const r = await fetch("/api/dashboard", { cache: "no-store" });
  if (!r.ok) throw new Error("dashboard fetch failed");
  return r.json();
}

export async function fetchReports(params: Record<string, string>) {
  const q = new URLSearchParams(params);
  const r = await fetch(`/api/reports?${q}`, { cache: "no-store" });
  return r.json();
}

export async function fetchEvents(limit = 80) {
  const r = await fetch(`/api/events?limit=${limit}`, { cache: "no-store" });
  return r.json();
}

/** Match dashboard API: enough points for full window at ~1–2 Hz (see series_limit_for_slug). */
export function seriesLimitForSlug(slug: string): number {
  const m = slug.toLowerCase().match(/-updown-(\d+)m-/);
  if (m) return Math.min(3600, Math.max(300, parseInt(m[1], 10) * 60 * 3));
  const h = slug.toLowerCase().match(/-updown-(\d+)h-/);
  if (h) return Math.min(3600, Math.max(300, parseInt(h[1], 10) * 3600 * 3));
  return 1200;
}

export async function fetchSeries(slug: string) {
  const limit = seriesLimitForSlug(slug);
  const r = await fetch(`/api/series?slug=${encodeURIComponent(slug)}&limit=${limit}`, {
    cache: "no-store",
  });
  return r.json();
}

export async function fetchOrders() {
  const r = await fetch("/api/orders?limit=50", { cache: "no-store" });
  return r.json();
}

export async function fetchFills(opts?: { limit?: number; slug?: string }) {
  const q = new URLSearchParams();
  q.set("limit", String(opts?.limit ?? 50));
  if (opts?.slug) q.set("slug", opts.slug);
  const r = await fetch(`/api/fills?${q}`, { cache: "no-store" });
  return r.json();
}

export async function fetchAnalyticsHours() {
  const r = await fetch("/api/analytics/hours", { cache: "no-store" });
  return r.json();
}

export async function fetchAnalyticsDaily(days = 30) {
  const r = await fetch(`/api/analytics/daily?days=${days}`, { cache: "no-store" });
  return r.json();
}

export async function fetchAnalyticsBuckets() {
  const r = await fetch("/api/analytics/buckets", { cache: "no-store" });
  return r.json();
}

export async function activateKill() {
  await fetch("/api/kill", { method: "POST" });
}

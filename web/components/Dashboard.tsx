"use client";

import clsx from "clsx";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
  CartesianGrid,
  BarChart,
  Bar,
  Cell,
  Legend,
} from "recharts";
import { useLiveClobBooks } from "../lib/clobLive";
import { viewSnapshot, pickWindow } from "../lib/view";
import {
  activateKill,
  fetchAnalyticsBuckets,
  fetchAnalyticsDaily,
  fetchAnalyticsHours,
  fetchDashboard,
  fetchEvents,
  fetchFills,
  fetchOrders,
  fetchReports,
  fetchSeries,
  type DashboardPayload,
} from "../lib/api";

type Tab =
  | "overview"
  | "live"
  | "chart"
  | "positions"
  | "reports"
  | "performance"
  | "hours"
  | "volume"
  | "buckets"
  | "risk"
  | "ops";

const TABS: { id: Tab; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "live", label: "Live market" },
  { id: "chart", label: "Price graph" },
  { id: "positions", label: "Orders & fills" },
  { id: "reports", label: "Market reports" },
  { id: "performance", label: "Performance" },
  { id: "hours", label: "Active hours" },
  { id: "volume", label: "Daily volume" },
  { id: "buckets", label: "Price buckets" },
  { id: "risk", label: "Risk & controls" },
  { id: "ops", label: "System / ops" },
];

export default function Dashboard() {
  const [tab, setTab] = useState<Tab>("overview");
  const [data, setData] = useState<DashboardPayload | null>(null);
  const [reports, setReports] = useState<Record<string, unknown>[]>([]);
  const [events, setEvents] = useState<Record<string, unknown>[]>([]);
  const [orders, setOrders] = useState<Record<string, unknown>[]>([]);
  const [fills, setFills] = useState<Record<string, unknown>[]>([]);
  const [series, setSeries] = useState<Record<string, unknown>[]>([]);
  const [hours, setHours] = useState<Record<string, unknown>[]>([]);
  const [daily, setDaily] = useState<Record<string, unknown>[]>([]);
  const [buckets, setBuckets] = useState<Record<string, unknown>[]>([]);
  const [viewTf, setViewTf] = useState("5m");
  const [reportAsset, setReportAsset] = useState("");
  const [reportTf, setReportTf] = useState("");
  const [reportResult, setReportResult] = useState("");
  const [selectedSlug, setSelectedSlug] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [killMsg, setKillMsg] = useState<string | null>(null);

  const rawSnap = (data?.snapshot ?? {}) as Record<string, unknown>;
  const snap = viewSnapshot(rawSnap, viewTf);
  const market = snap.current_market as Record<string, unknown> | undefined;
  const slug = (market?.slug as string) ?? "";
  const viewActive = Boolean(snap.view_active);

  const refresh = useCallback(async () => {
    try {
      setError(null);
      const d = await fetchDashboard();
      setData(d);
    } catch {
      setError("Cannot reach dashboard API. Run make dashboard.");
    }
  }, []);

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 5000);
    return () => clearInterval(t);
  }, [refresh]);

  useEffect(() => {
    if (tab === "reports" || tab === "performance") {
      const p: Record<string, string> = { limit: "100" };
      if (reportAsset) p.asset = reportAsset;
      if (reportTf) p.timeframe = reportTf;
      if (reportResult) p.result = reportResult;
      fetchReports(p).then((r) => setReports(r.reports ?? []));
    }
    if (tab === "ops") fetchEvents(120).then((r) => setEvents(r.events ?? []));
    if (tab === "positions") {
      fetchOrders().then((r) => setOrders(r.orders ?? []));
      fetchFills().then((r) => setFills(r.fills ?? []));
    }
    if (tab === "hours") fetchAnalyticsHours().then((r) => setHours(r.hours ?? []));
    if (tab === "volume") fetchAnalyticsDaily(30).then((r) => setDaily(r.days ?? []));
    if (tab === "buckets") fetchAnalyticsBuckets().then((r) => setBuckets(r.buckets ?? []));
  }, [tab, reportAsset, reportTf, reportResult]);

  const seriesSlug = selectedSlug ?? slug;

  const loadSeries = useCallback(() => {
    if (!seriesSlug) {
      setSeries([]);
      return;
    }
    fetchSeries(seriesSlug).then((r) => setSeries(r.points ?? []));
  }, [seriesSlug]);

  useEffect(() => {
    loadSeries();
  }, [loadSeries, viewTf]);

  useEffect(() => {
    if (tab !== "chart") return;
    loadSeries();
    const t = setInterval(loadSeries, 5000);
    return () => clearInterval(t);
  }, [tab, loadSeries]);

  async function onKill() {
    if (!window.confirm("Activate kill switch? Engine will stop new orders.")) return;
    try {
      await activateKill();
      setKillMsg("Kill switch file written. Engine will halt trading.");
      refresh();
    } catch {
      setKillMsg("Kill switch request failed.");
    }
  }

  const chartData = useMemo(() => {
    const fromSeries = series.map((p) => {
      const pt = (p.point ?? {}) as Record<string, string>;
      const ts = String(p.ts ?? "");
      return {
        t: ts.slice(11, 19),
        beat: num(pt.beat),
        spot: num(pt.chainlink_spot),
        twap: num(pt.chainlink_twap),
        coinbase: num(pt.coinbase_spot),
        binance: num(pt.binance_spot),
      };
    });
    if (fromSeries.length > 0) return fromSeries;

    const sig = (snap.signal ?? {}) as Record<string, unknown>;
    const feeds = (snap.feeds ?? {}) as Record<string, unknown>;
    const clSpot = (sig.chainlink_spot as Record<string, unknown>)?.px ?? (feeds.chainlink_spot as Record<string, unknown>)?.px;
    const clTwap = (sig.chainlink_twap as Record<string, unknown>)?.px ?? (feeds.chainlink_twap as Record<string, unknown>)?.px;
    const cb = (sig.coinbase_spot as Record<string, unknown>)?.px ?? (feeds.coinbase_spot as Record<string, unknown>)?.px;
    const bn = (sig.binance_spot as Record<string, unknown>)?.px ?? (feeds.binance_spot as Record<string, unknown>)?.px;
    const beat = sig.beat ?? (snap.current_market as Record<string, unknown> | undefined)?.beat;
    if (!clSpot && !clTwap && !beat) return [];

    return [
      {
        t: new Date().toISOString().slice(11, 19),
        beat: num(String(beat ?? "")),
        spot: num(String(clSpot ?? "")),
        twap: num(String(clTwap ?? "")),
        coinbase: num(String(cb ?? "")),
        binance: num(String(bn ?? "")),
      },
    ];
  }, [series, snap]);

  const engineState = String(snap.engine_state ?? "unknown");
  const dryRun = Boolean(snap.dry_run);

  return (
    <div className="dashboard-shell">
      <aside className="sidebar">
        <h1>Polymarket Crypto SDK</h1>
        <p>Read-only admin · engine telemetry</p>
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            className={clsx("nav-btn", tab === t.id && "active")}
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </aside>
      <div className="main">
        <header style={{ display: "flex", flexWrap: "wrap", gap: 12, alignItems: "center", marginBottom: 20 }}>
          <span className={clsx("badge", engineState)}>{engineState.toUpperCase()}</span>
          {dryRun && <span className="badge dry">DRY RUN</span>}
          {Boolean((data?.heartbeat as Record<string, unknown>)?.kill) && (
            <span className="badge killed">KILL ON</span>
          )}
          <span style={{ color: "var(--muted)", fontSize: 13 }}>
            {viewActive ? slug : `No active ${viewTf} window`}
            {" · "}
            view {viewTf} · engine snapshot 5s · CLOB books live (WS)
          </span>
          {error && <span style={{ color: "var(--red)", fontSize: 13 }}>{error}</span>}
          {killMsg && <span style={{ color: "var(--amber)", fontSize: 13 }}>{killMsg}</span>}
        </header>

        <ViewBar viewTf={viewTf} onViewTf={setViewTf} config={(rawSnap.config ?? data?.config_meta) as Record<string, unknown>} />

        {(tab === "reports" || tab === "performance") && (
          <ReportFiltersBar
            asset={reportAsset}
            tf={reportTf}
            result={reportResult}
            onAsset={setReportAsset}
            onTf={setReportTf}
            onResult={setReportResult}
          />
        )}

        {tab === "overview" && <Overview data={data} snap={snap} rawSnap={rawSnap} viewTf={viewTf} onKill={onKill} />}
        {tab === "live" && <LiveMarket snap={snap} viewTf={viewTf} />}
        {tab === "chart" && <PriceChart chartData={chartData} slug={selectedSlug ?? slug} />}
        {tab === "positions" && <Positions orders={orders} fills={fills} snap={snap} />}
        {tab === "reports" && (
          <ReportsTable reports={reports} onSelect={(s) => { setSelectedSlug(s); setTab("chart"); }} />
        )}
        {tab === "performance" && <Performance pnl={data?.pnl} reports={reports} />}
        {tab === "hours" && <HoursChart hours={hours} />}
        {tab === "volume" && <DailyChart daily={daily} />}
        {tab === "buckets" && <BucketsChart buckets={buckets} />}
        {tab === "risk" && <RiskPanel data={data} snap={snap} onKill={onKill} />}
        {tab === "ops" && <OpsPanel snap={snap} events={events} />}
      </div>
    </div>
  );
}

function ViewBar(props: {
  viewTf: string;
  onViewTf: (v: string) => void;
  config?: Record<string, unknown>;
}) {
  const tfs = (props.config?.timeframes as string[] | undefined) ?? ["5m", "15m"];
  return (
    <div className="filters">
      <span className="metric-label" style={{ alignSelf: "center" }}>View timeframe</span>
      {tfs.map((tf) => (
        <button
          key={tf}
          type="button"
          className={clsx("nav-btn", props.viewTf === tf && "active")}
          style={{ width: "auto", display: "inline-block" }}
          onClick={() => props.onViewTf(tf)}
        >
          {tf}
        </button>
      ))}
    </div>
  );
}

function ReportFiltersBar(props: {
  asset: string;
  tf: string;
  result: string;
  onAsset: (v: string) => void;
  onTf: (v: string) => void;
  onResult: (v: string) => void;
}) {
  return (
    <div className="filters">
      <span className="metric-label" style={{ alignSelf: "center" }}>Report filters</span>
      <select value={props.asset} onChange={(e) => props.onAsset(e.target.value)}>
        <option value="">All assets</option>
        <option value="btc">BTC</option>
        <option value="eth">ETH</option>
      </select>
      <select value={props.tf} onChange={(e) => props.onTf(e.target.value)}>
        <option value="">All timeframes</option>
        <option value="5m">5m</option>
        <option value="15m">15m</option>
      </select>
      <select value={props.result} onChange={(e) => props.onResult(e.target.value)}>
        <option value="">All results</option>
        <option value="win">Win</option>
        <option value="loss">Loss</option>
        <option value="scratch">Scratch</option>
      </select>
    </div>
  );
}

function Overview({
  data,
  snap,
  rawSnap,
  viewTf,
  onKill,
}: {
  data: DashboardPayload | null;
  snap: Record<string, unknown>;
  rawSnap: Record<string, unknown>;
  viewTf: string;
  onKill: () => void;
}) {
  const other = viewTf === "5m" ? "15m" : "5m";
  const otherWin = pickWindow(rawSnap, other);
  const feeds = (snap.feeds ?? {}) as Record<string, unknown>;
  const spot = (feeds.chainlink_spot ?? {}) as Record<string, unknown>;
  const twap = (feeds.chainlink_twap ?? {}) as Record<string, unknown>;
  const coinbase = (feeds.coinbase_spot ?? {}) as Record<string, unknown>;
  const binance = (feeds.binance_spot ?? {}) as Record<string, unknown>;
  const pnl = data?.pnl;

  return (
    <>
      <div className="grid-4" style={{ marginBottom: 16 }}>
        <MetricCard label="Total PnL" value={pnl?.total_pnl ?? "0"} />
        <MetricCard label="Win rate" value={pct(pnl?.win_rate)} />
        <MetricCard label="Trades" value={String(pnl?.trade_count ?? 0)} />
        <MetricCard label="Avg PnL / market" value={pnl?.avg_pnl_per_market ?? "—"} />
      </div>
      <div className="grid-2">
        <div className="card">
          <h3>Engine & feeds</h3>
          <FeedRow name="Chainlink spot" freshness={String(spot.freshness)} age={spot.age_ms} />
          <FeedRow name="Chainlink TWAP" freshness={String(twap.freshness)} age={twap.age_ms} />
          <FeedRow name="Coinbase spot" freshness={String(coinbase.freshness ?? "unknown")} age={coinbase.age_ms} />
          <FeedRow name="Binance spot" freshness={String(binance.freshness ?? "unknown")} age={binance.age_ms} />
          <WsHealth feeds={feeds} />
          <p className="metric-label" style={{ marginTop: 12 }}>
            Uptime {String(snap.uptime_sec ?? 0)}s · region {String(snap.region ?? "—")}
          </p>
        </div>
        <div className="card">
          <h3>Current window ({viewTf})</h3>
          {snap.view_active ? (
            <MarketSummary market={snap.current_market as Record<string, unknown>} />
          ) : (
            <p className="empty">No active {viewTf} market in engine. {otherWin?.active ? `Try ${other}.` : "Waiting for lifecycle…"}</p>
          )}
          <NextMarket next={snap.next_market as Record<string, unknown>} />
          {otherWin?.active && (
            <p className="metric-label">Also live: {String((otherWin.market as Record<string, unknown>)?.slug ?? other)}</p>
          )}
          <button type="button" className="btn-danger" style={{ marginTop: 16 }} onClick={onKill}>
            Activate kill switch
          </button>
        </div>
      </div>
    </>
  );
}

function useMarketTokens(
  slug: string | undefined,
  tokenUp?: string,
  tokenDown?: string,
): { tokenUp?: string; tokenDown?: string } {
  const [resolved, setResolved] = useState<{ tokenUp?: string; tokenDown?: string }>({
    tokenUp,
    tokenDown,
  });

  useEffect(() => {
    if (tokenUp && tokenDown) {
      setResolved({ tokenUp, tokenDown });
      return;
    }
    if (!slug) return;
    let cancelled = false;
    void (async () => {
      try {
        const r = await fetch(`https://gamma-api.polymarket.com/events?slug=${encodeURIComponent(slug)}`, {
          cache: "no-store",
        });
        if (!r.ok) return;
        const data = (await r.json()) as Record<string, unknown>[];
        const ev = data[0];
        const markets = (ev?.markets as Record<string, unknown>[]) ?? [];
        const m = markets[0];
        const parseList = (v: unknown): string[] => {
          if (Array.isArray(v)) return v.map(String);
          if (typeof v === "string") {
            try {
              const p = JSON.parse(v) as unknown;
              return Array.isArray(p) ? p.map(String) : [];
            } catch {
              return [];
            }
          }
          return [];
        };
        const ids = parseList(m?.clobTokenIds);
        const outcomes = parseList(m?.outcomes).map((o) => o.toLowerCase());
        if (ids.length < 2) return;
        let up = ids[0];
        let down = ids[1];
        for (let i = 0; i < outcomes.length && i < ids.length; i++) {
          const o = outcomes[i];
          if (o === "up" || o === "yes") up = ids[i];
          if (o === "down" || o === "no") down = ids[i];
        }
        if (!cancelled) setResolved({ tokenUp: up, tokenDown: down });
      } catch {
        /* ignore */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [slug, tokenUp, tokenDown]);

  return resolved;
}

function LiveMarket({
  snap,
  viewTf,
}: {
  snap: Record<string, unknown>;
  viewTf: string;
}) {
  if (!snap.view_active) {
    return <p className="empty">No active {viewTf} window.</p>;
  }
  const market = snap.current_market as Record<string, unknown> | undefined;
  const strat = (snap.strategy ?? {}) as Record<string, unknown>;
  const signal = (snap.signal ?? {}) as Record<string, unknown>;
  const resolvedTokens = useMarketTokens(
    market?.slug as string | undefined,
    market?.token_up as string | undefined,
    market?.token_down as string | undefined,
  );
  const live = useLiveClobBooks(resolvedTokens.tokenUp, resolvedTokens.tokenDown);
  const upFallback = (signal.up_bbo ?? {}) as Record<string, unknown>;
  const downFallback = (signal.down_bbo ?? {}) as Record<string, unknown>;
  const up = live.up.valid ? live.up : upFallback;
  const down = live.down.valid ? live.down : downFallback;

  return (
    <div className="grid-2">
      <div className="card">
        <h3>Market timing</h3>
        <MarketSummary market={market} />
        <p>Countdown to entry: {String(strat.secs_until_entry ?? "—")}s (after {String(strat.entry_after_sec ?? "—")}s)</p>
        <p>Strategy: <strong>{String(strat.state ?? "—")}</strong></p>
        <p>TWAP − beat: {String(strat.twap_minus_beat ?? "—")} (threshold {String(strat.twap_beat_diff_usd ?? "—")})</p>
        <p>Best ask &gt; min: {String(strat.best_ask_ok ?? "—")}</p>
        <p>Last decision: {JSON.stringify(snap.last_decision ?? null)}</p>
      </div>
      <div className="card">
        <h3>Books & prices</h3>
        <p className="metric-label" style={{ marginBottom: 8 }}>
          CLOB market WS {live.connected ? "connected" : "connecting…"}
          {(live.up.live || live.down.live) && " · streaming"}
        </p>
        <BboRow side="Up" bbo={up} />
        <BboRow side="Down" bbo={down} />
        <p className="metric-label" style={{ marginTop: 12, marginBottom: 4 }}>
          Up / Down best ask (0–1) · live CLOB
        </p>
        <div className="chart-wrap" style={{ height: 220 }}>
          <LiveAskChart data={live.askSeries} />
        </div>
      </div>
    </div>
  );
}

function LiveAskChart({ data }: { data: { t: string; upAsk?: number; downAsk?: number }[] }) {
  if (!data.length) {
    return <p className="empty">Waiting for CLOB ask updates…</p>;
  }
  return (
    <ResponsiveContainer width="100%" height="100%">
      <LineChart data={data}>
        <CartesianGrid stroke="#1e2836" strokeDasharray="3 3" />
        <XAxis dataKey="t" stroke="#8b9cb3" fontSize={11} minTickGap={28} />
        <YAxis stroke="#8b9cb3" fontSize={11} domain={[0, 1]} tickFormatter={(v) => Number(v).toFixed(2)} />
        <Tooltip
          contentStyle={{ background: "#121820", border: "1px solid #1e2836" }}
          formatter={(v: number) => v.toFixed(4)}
        />
        <Legend />
        <Line type="monotone" dataKey="upAsk" name="Up ask" stroke="#f5a524" dot={false} strokeWidth={1.5} />
        <Line type="monotone" dataKey="downAsk" name="Down ask" stroke="#f04438" dot={false} strokeWidth={1.5} />
      </LineChart>
    </ResponsiveContainer>
  );
}

function PriceChart({
  chartData,
  slug,
}: {
  chartData: { t: string; beat?: number; spot?: number; twap?: number; coinbase?: number; binance?: number }[];
  slug: string;
}) {
  const hasCb = chartData.some((p) => (p.coinbase ?? 0) > 0);
  const hasBn = chartData.some((p) => (p.binance ?? 0) > 0);
  return (
    <div className="card">
      <h3>Live window · {slug || "—"}</h3>
      <p className="metric-label" style={{ marginBottom: 12 }}>
        BTC USD: Chainlink beat / spot / TWAP plus optional Coinbase &amp; Binance spot (1 Hz slot samples). CLOB asks on Live market.
      </p>
      {chartData.length === 0 ? (
        <p className="empty">No samples yet — engine writes 1Hz slot samples when a market is active.</p>
      ) : (
        <div className="chart-wrap">
          <ResponsiveContainer width="100%" height="100%">
            <LineChart data={chartData}>
              <CartesianGrid stroke="#1e2836" strokeDasharray="3 3" />
              <XAxis dataKey="t" stroke="#8b9cb3" fontSize={11} />
              <YAxis stroke="#8b9cb3" fontSize={11} domain={["auto", "auto"]} />
              <Tooltip contentStyle={{ background: "#121820", border: "1px solid #1e2836" }} />
              <Legend />
              <Line type="monotone" dataKey="beat" name="Beat" stroke="#7c5cff" dot={false} strokeWidth={2} />
              <Line type="monotone" dataKey="spot" name="CL spot" stroke="#4da3ff" dot={false} strokeWidth={1.5} />
              <Line type="monotone" dataKey="twap" name="CL TWAP" stroke="#3dd68c" dot={false} strokeWidth={1.5} />
              {hasCb && (
                <Line type="monotone" dataKey="coinbase" name="Coinbase" stroke="#f7931a" dot={false} strokeWidth={1.5} />
              )}
              {hasBn && (
                <Line type="monotone" dataKey="binance" name="Binance" stroke="#f0b90b" dot={false} strokeWidth={1.5} strokeDasharray="4 2" />
              )}
            </LineChart>
          </ResponsiveContainer>
        </div>
      )}
    </div>
  );
}

function Positions({
  orders,
  fills,
  snap,
}: {
  orders: Record<string, unknown>[];
  fills: Record<string, unknown>[];
  snap: Record<string, unknown>;
}) {
  const market = snap.current_market as Record<string, unknown> | undefined;
  const presign = market?.presign as Record<string, unknown> | undefined;

  return (
    <div className="grid-2">
      <div className="card">
        <h3>Working orders</h3>
        {orders.length === 0 ? <p className="empty">No orders in DB (live POST not persisted yet).</p> : (
          <DataTable
            columns={["slug", "side", "signed_px", "size", "order_type", "status"]}
            rows={orders}
          />
        )}
      </div>
      <div className="card">
        <h3>Fills</h3>
        {fills.length === 0 ? <p className="empty">No fills recorded yet.</p> : (
          <DataTable columns={["slug", "side", "px", "size", "fee", "ts"]} rows={fills} />
        )}
        <h3 style={{ marginTop: 20 }}>Presign cache</h3>
        <p>
          {presign ? `${presign.ready}/${presign.total} ready · tick ${presign.tick}` : "—"}
        </p>
        <p className="metric-label">Wallet USDC: {String((snap.wallet as Record<string, unknown>)?.usdc ?? "0")}</p>
      </div>
    </div>
  );
}

function ReportsTable({
  reports,
  onSelect,
}: {
  reports: Record<string, unknown>[];
  onSelect: (slug: string) => void;
}) {
  return (
    <div className="card">
      <h3>Closed windows</h3>
      {reports.length === 0 ? (
        <p className="empty">No closed market reports yet.</p>
      ) : (
        <table className="data">
          <thead>
            <tr>
              <th>Slug</th>
              <th>Result</th>
              <th>PnL</th>
              <th>Net</th>
              <th>Beat</th>
              <th>TWAP−beat</th>
              <th>Dry</th>
            </tr>
          </thead>
          <tbody>
            {reports.map((r) => (
              <tr key={String(r.slug)} style={{ cursor: "pointer" }} onClick={() => onSelect(String(r.slug))}>
                <td>{String(r.slug)}</td>
                <td>{String(r.result)}</td>
                <td>{String(r.pnl)}</td>
                <td>{String(r.net)}</td>
                <td>{String(r.beat ?? "—")}</td>
                <td>{String(r.twap_minus_beat ?? "—")}</td>
                <td>{String(r.dry_run)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

function Performance({ pnl, reports }: { pnl?: DashboardPayload["pnl"]; reports: Record<string, unknown>[] }) {
  const cumulative = useMemo(() => {
    let sum = 0;
    return [...reports].reverse().map((r, i) => {
      sum += parseFloat(String(r.pnl ?? 0));
      return { i, pnl: sum };
    });
  }, [reports]);

  return (
    <div className="grid-2">
      <div className="card">
        <h3>Summary</h3>
        <MetricCard label="Total PnL" value={pnl?.total_pnl ?? "0"} />
        <p>Win rate {pct(pnl?.win_rate)} · trades {pnl?.trade_count ?? 0}</p>
        <p>Avg winner {pnl?.avg_winner ?? "—"} · avg loser {pnl?.avg_loser ?? "—"}</p>
      </div>
      <div className="card">
        <h3>Cumulative PnL</h3>
        <div className="chart-wrap">
          <ResponsiveContainer width="100%" height="100%">
            <LineChart data={cumulative}>
              <CartesianGrid stroke="#1e2836" />
              <XAxis dataKey="i" hide />
              <YAxis stroke="#8b9cb3" fontSize={11} />
              <Tooltip contentStyle={{ background: "#121820", border: "1px solid #1e2836" }} />
              <Line type="monotone" dataKey="pnl" stroke="#3dd68c" dot={false} />
            </LineChart>
          </ResponsiveContainer>
        </div>
      </div>
    </div>
  );
}

function HoursChart({ hours }: { hours: Record<string, unknown>[] }) {
  const data = hours.map((h) => ({
    hour: `${h.hour}h`,
    trades: Number(h.trades ?? 0),
    pnl: parseFloat(String(h.pnl ?? 0)),
  }));
  return (
    <div className="card">
      <h3>Trades & PnL by UTC hour</h3>
      {data.length === 0 ? (
        <p className="empty">No data</p>
      ) : (
        <div className="chart-wrap">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={data}>
              <CartesianGrid stroke="#1e2836" />
              <XAxis dataKey="hour" stroke="#8b9cb3" fontSize={11} />
              <YAxis stroke="#8b9cb3" fontSize={11} />
              <Tooltip contentStyle={{ background: "#121820", border: "1px solid #1e2836" }} />
              <Bar dataKey="trades" fill="#4da3ff" />
            </BarChart>
          </ResponsiveContainer>
        </div>
      )}
    </div>
  );
}

function DailyChart({ daily }: { daily: Record<string, unknown>[] }) {
  const data = daily.map((d) => ({
    day: String(d.day),
    net: parseFloat(String(d.net_pnl ?? 0)),
    fees: parseFloat(String(d.fees ?? 0)),
  }));
  return (
    <div className="card">
      <h3>Daily net PnL & fees (30d)</h3>
      <div className="chart-wrap">
        <ResponsiveContainer width="100%" height="100%">
          <BarChart data={data}>
            <CartesianGrid stroke="#1e2836" />
            <XAxis dataKey="day" stroke="#8b9cb3" fontSize={10} />
            <YAxis stroke="#8b9cb3" fontSize={11} />
            <Tooltip contentStyle={{ background: "#121820", border: "1px solid #1e2836" }} />
            <Bar dataKey="net" fill="#3dd68c" />
            <Bar dataKey="fees" fill="#f5a524" />
          </BarChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}

function BucketsChart({ buckets }: { buckets: Record<string, unknown>[] }) {
  const colors = ["#4da3ff", "#7c5cff", "#3dd68c", "#f5a524", "#f04438"];
  return (
    <div className="card">
      <h3>PnL by entry price bucket</h3>
      {buckets.length === 0 ? (
        <p className="empty">No bucket data (needs signed_px on reports).</p>
      ) : (
        <div className="chart-wrap">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={buckets}>
              <CartesianGrid stroke="#1e2836" />
              <XAxis dataKey="bucket" stroke="#8b9cb3" fontSize={11} />
              <YAxis stroke="#8b9cb3" fontSize={11} />
              <Tooltip contentStyle={{ background: "#121820", border: "1px solid #1e2836" }} />
              <Bar dataKey="pnl">
                {buckets.map((_, i) => (
                  <Cell key={i} fill={colors[i % colors.length]} />
                ))}
              </Bar>
            </BarChart>
          </ResponsiveContainer>
        </div>
      )}
    </div>
  );
}

function RiskPanel({
  data,
  snap,
  onKill,
}: {
  data: DashboardPayload | null;
  snap: Record<string, unknown>;
  onKill: () => void;
}) {
  const risk = (snap.risk ?? {}) as Record<string, unknown>;
  const cfg = data?.config_meta ?? {};
  return (
    <div className="grid-2">
      <div className="card">
        <h3>Limits</h3>
        <p>Hourly {String(risk.hourly_used)} / {String(risk.hourly_cap)}</p>
        <p>Daily {String(risk.daily_used)} / {String(risk.daily_cap)}</p>
        <p>Orders this market {String(risk.orders_this_market)} / {String(risk.max_orders_per_market)}</p>
        <button type="button" className="btn-danger" onClick={onKill}>Activate kill switch</button>
      </div>
      <div className="card">
        <h3>Config snapshot</h3>
        <pre style={{ fontSize: 12, overflow: "auto" }}>{JSON.stringify(cfg, null, 2)}</pre>
      </div>
    </div>
  );
}

function OpsPanel({ snap, events }: { snap: Record<string, unknown>; events: Record<string, unknown>[] }) {
  const alerts = (snap.alerts ?? []) as unknown[];
  return (
    <div className="grid-2">
      <div className="card">
        <h3>Alerts</h3>
        {alerts.length === 0 ? <p className="empty">No active alerts</p> : (
          <ul>{alerts.map((a, i) => <li key={i}>{JSON.stringify(a)}</li>)}</ul>
        )}
      </div>
      <div className="card">
        <h3>Event log tail</h3>
        <div style={{ maxHeight: 400, overflow: "auto", fontSize: 12 }}>
          {events.map((e, i) => (
            <div key={i} style={{ borderBottom: "1px solid var(--panel-border)", padding: "6px 0" }}>
              <strong>{String(e.kind)}</strong> {String(e.ts)}
              <pre style={{ margin: 4, whiteSpace: "pre-wrap" }}>{JSON.stringify(e.payload, null, 0)}</pre>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function MetricCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="card">
      <div className="metric-label">{label}</div>
      <div className="metric-value">{value}</div>
    </div>
  );
}

function FeedRow({ name, freshness, age }: { name: string; freshness: string; age: unknown }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
      <span className={clsx("freshness", freshness)} />
      <span>{name}</span>
      <span style={{ color: "var(--muted)", marginLeft: "auto" }}>{formatAge(age)}</span>
    </div>
  );
}

function WsHealth({ feeds }: { feeds: Record<string, unknown> }) {
  const ws = (feeds.ws ?? {}) as Record<string, string>;
  return (
    <div style={{ marginTop: 12, fontSize: 13 }}>
      <p className="metric-label">WS / optional feeds</p>
      {Object.entries(ws).map(([k, v]) => (
        <div key={k}>{k}: {v}</div>
      ))}
    </div>
  );
}

function MarketSummary({ market }: { market?: Record<string, unknown> }) {
  if (!market) return <p className="empty">No active market</p>;
  return (
    <>
      <p><strong>{String(market.slug)}</strong></p>
      <p>{String(market.asset)} · {String(market.timeframe)} · {String(market.secs_left ?? "—")}s left</p>
      <p>Beat {String(market.beat ?? "—")}</p>
    </>
  );
}

function NextMarket({ next }: { next?: Record<string, unknown> }) {
  if (!next) return null;
  return (
    <p className="metric-label" style={{ marginTop: 8 }}>
      Next: {String(next.slug)} · presign {String(next.presign_ready ?? false)}
    </p>
  );
}

function BboRow({ side, bbo }: { side: string; bbo: Record<string, unknown> }) {
  const bid = parseFloat(String(bbo.bid ?? "0"));
  const ask = parseFloat(String(bbo.ask ?? "0"));
  const hasPx = bid > 0 || ask > 0;
  const valid = bbo.valid !== false && hasPx;
  const live = bbo.live === true;
  const stale = bbo.stale === true;
  const badge = live ? (
    <span className="badge running" style={{ marginLeft: 8 }}>
      live
    </span>
  ) : !valid || stale ? (
    <span className="badge warn" style={{ marginLeft: 8 }}>
      {!hasPx ? "no book" : "stale"}
    </span>
  ) : null;
  return (
    <p>
      {side}: bid {String(bbo.bid)} ({String(bbo.bid_sz)}) · ask {String(bbo.ask)} ({String(bbo.ask_sz)}) · spread{" "}
      {String(bbo.spread)}
      {badge}
    </p>
  );
}

function DataTable({ columns, rows }: { columns: string[]; rows: Record<string, unknown>[] }) {
  return (
    <table className="data">
      <thead>
        <tr>{columns.map((c) => <th key={c}>{c}</th>)}</tr>
      </thead>
      <tbody>
        {rows.map((r, i) => (
          <tr key={i}>{columns.map((c) => <td key={c}>{String(r[c] ?? "—")}</td>)}</tr>
        ))}
      </tbody>
    </table>
  );
}

function pct(v?: number) {
  if (v == null) return "—";
  return `${(v * 100).toFixed(1)}%`;
}

function num(v?: string) {
  if (!v) return undefined;
  const n = parseFloat(v);
  return Number.isFinite(n) ? n : undefined;
}

function formatAge(age: unknown) {
  if (age == null || age === "") return "no data";
  const n = Number(age);
  if (!Number.isFinite(n) || n < 0) return "no data";
  return `${Math.round(n)}ms`;
}

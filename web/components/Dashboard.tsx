"use client";

import clsx from "clsx";
import { memo, useCallback, useEffect, useMemo, useState } from "react";
import {
  Area,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
  CartesianGrid,
  BarChart,
  Bar,
  Legend,
  ReferenceLine,
  ReferenceDot,
  type TooltipContentProps,
} from "recharts";
import { useLiveClobBooks } from "../lib/clobLive";
import { viewSnapshot, configuredAssets, assetWindowsForTf } from "../lib/view";
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
import AnalyticsHub from "./AnalyticsHub";
import { formatUsd, pnlClass } from "../lib/analytics";

type Tab =
  | "overview"
  | "live"
  | "chart"
  | "positions"
  | "reports"
  | "analytics"
  | "risk"
  | "ops";

const NAV_GROUPS: { label: string; items: { id: Tab; label: string }[] }[] = [
  {
    label: "Trading",
    items: [
      { id: "overview", label: "Overview" },
      { id: "live", label: "Live market" },
      { id: "chart", label: "Price graph" },
    ],
  },
  {
    label: "History",
    items: [
      { id: "positions", label: "Orders & fills" },
      { id: "reports", label: "Market reports" },
    ],
  },
  {
    label: "Insights",
    items: [{ id: "analytics", label: "Analytics" }],
  },
  {
    label: "System",
    items: [
      { id: "risk", label: "Risk & controls" },
      { id: "ops", label: "Logs & alerts" },
    ],
  },
];

const TRADING_TABS = new Set<Tab>(["overview", "live", "chart"]);

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
  const [viewAsset, setViewAsset] = useState("");
  const [reportAsset, setReportAsset] = useState("");
  const [reportTf, setReportTf] = useState("");
  const [reportResult, setReportResult] = useState("");
  const [selectedSlug, setSelectedSlug] = useState<string | null>(null);
  const [chartFills, setChartFills] = useState<Record<string, unknown>[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [killMsg, setKillMsg] = useState<string | null>(null);

  const rawSnap = (data?.snapshot ?? {}) as Record<string, unknown>;
  const configBundle = (rawSnap.config ?? data?.config_meta) as Record<string, unknown> | undefined;
  const assetsList = useMemo(
    () => configuredAssets(rawSnap, data?.config_meta as Record<string, unknown> | undefined),
    [rawSnap, data?.config_meta],
  );
  const snap = viewSnapshot(rawSnap, viewTf, viewAsset || assetsList[0] || "eth");
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
    if (viewAsset && assetsList.includes(viewAsset)) return;
    const rows = assetWindowsForTf(rawSnap, viewTf, assetsList);
    const firstActive = rows.find((r) => r.active)?.asset;
    setViewAsset(firstActive ?? assetsList[0] ?? "eth");
  }, [rawSnap, viewTf, assetsList, viewAsset]);

  useEffect(() => {
    if (tab === "reports" || tab === "analytics") {
      const p: Record<string, string> = { limit: "200" };
      if (reportAsset) p.asset = reportAsset;
      if (reportTf) p.timeframe = reportTf;
      if (reportResult) p.result = reportResult;
      fetchReports(p).then((r) => setReports(r.reports ?? []));
    }
    if (tab === "analytics") {
      fetchAnalyticsHours().then((r) => setHours(r.hours ?? []));
      fetchAnalyticsDaily(30).then((r) => setDaily(r.days ?? []));
      fetchAnalyticsBuckets().then((r) => setBuckets(r.buckets ?? []));
    }
    if (tab === "ops") fetchEvents(120).then((r) => setEvents(r.events ?? []));
    if (tab === "positions") {
      fetchOrders().then((r) => setOrders(r.orders ?? []));
      fetchFills().then((r) => setFills(r.fills ?? []));
    }
  }, [tab, reportAsset, reportTf, reportResult]);

  const seriesSlug = selectedSlug ?? slug;
  const viewingPastWindow = Boolean(selectedSlug && slug && selectedSlug !== slug);

  const openCurrentPriceGraph = useCallback(() => {
    setSelectedSlug(null);
    setTab("chart");
  }, []);

  const loadSeries = useCallback(() => {
    if (!seriesSlug) {
      setSeries([]);
      return;
    }
    fetchSeries(seriesSlug).then((r) => setSeries(r.points ?? []));
  }, [seriesSlug]);

  useEffect(() => {
    loadSeries();
  }, [loadSeries, viewTf, viewAsset]);

  useEffect(() => {
    if (tab !== "chart") return;
    loadSeries();
    const t = setInterval(loadSeries, 5000);
    return () => clearInterval(t);
  }, [tab, loadSeries]);

  useEffect(() => {
    if (tab !== "chart" || !seriesSlug) {
      setChartFills([]);
      return;
    }
    const load = () => {
      fetchFills({ slug: seriesSlug, limit: 20 }).then((r) => setChartFills(r.fills ?? []));
    };
    load();
    const t = setInterval(load, 5000);
    return () => clearInterval(t);
  }, [tab, seriesSlug]);

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
    const raw = dedupeChartBySecond(
      series.map((p) => {
        const pt = (p.point ?? {}) as Record<string, string>;
        const ts = String(p.ts ?? "");
        const xMs = Date.parse(ts);
        return {
          t: ts.slice(11, 19),
          xMs: Number.isFinite(xMs) ? xMs : 0,
          tsIso: ts,
          beat: chartPrice(pt.beat),
          spot: chartPrice(pt.chainlink_spot),
          twap: chartPrice(pt.chainlink_twap),
          coinbase: chartPrice(pt.coinbase_spot),
          binance: chartPrice(pt.binance_spot),
          coinbaseTwap: chartPrice(pt.coinbase_twap60),
          binanceTwap: chartPrice(pt.binance_twap60),
          coinbaseMom: chartPct(pt.coinbase_momentum_pct),
          binanceMom: chartPct(pt.binance_momentum_pct),
        };
      }),
    );

    const sanitized = sanitizeChartSeries(raw, seriesSlug);
    const anchor = robustChartAnchor(sanitized);
    const windowBeat = resolveWindowBeat(
      series,
      seriesSlug,
      reports,
      snap.current_market as Record<string, unknown> | undefined,
      anchor,
    );

    const fromSeries = sanitized.map((p) => ({
      ...p,
      beat:
        beatMatchesPriceScale(p.beat, anchor) ??
        windowBeat ??
        undefined,
    }));
    if (fromSeries.length > 0) return padChartToWindow(fromSeries, seriesSlug);

    const sig = (snap.signal ?? {}) as Record<string, unknown>;
    const feeds = (snap.feeds ?? {}) as Record<string, unknown>;
    const clSpot = (sig.chainlink_spot as Record<string, unknown>)?.px ?? (feeds.chainlink_spot as Record<string, unknown>)?.px;
    const clTwap = (sig.chainlink_twap as Record<string, unknown>)?.px ?? (feeds.chainlink_twap as Record<string, unknown>)?.px;
    const cb = (sig.coinbase_spot as Record<string, unknown>)?.px ?? (feeds.coinbase_spot as Record<string, unknown>)?.px;
    const bn = (sig.binance_spot as Record<string, unknown>)?.px ?? (feeds.binance_spot as Record<string, unknown>)?.px;
    const beat = sig.beat ?? (snap.current_market as Record<string, unknown> | undefined)?.beat;
    if (!clSpot && !clTwap && !beat) return [];

    const nowIso = new Date().toISOString();
    const xMs = Date.parse(nowIso);
    return [
      {
        t: nowIso.slice(11, 19),
        xMs: Number.isFinite(xMs) ? xMs : 0,
        tsIso: nowIso,
        beat: chartPrice(String(beat ?? "")),
        spot: chartPrice(String(clSpot ?? "")),
        twap: chartPrice(String(clTwap ?? "")),
        coinbase: chartPrice(String(cb ?? "")),
        binance: chartPrice(String(bn ?? "")),
      },
    ];
  }, [series, snap, seriesSlug, reports]);

  const fillMarkers = useMemo(
    () => buildFillMarkers(chartData, chartFills),
    [chartData, chartFills],
  );

  const engineState = String(snap.engine_state ?? "unknown");
  const dryRun = Boolean(snap.dry_run);

  return (
    <div className="dashboard-shell">
      <aside className="sidebar">
        <div className="sidebar-brand">
          <h1>Crypto engine</h1>
          <p>Paper trading · read-only dashboard</p>
        </div>
        {NAV_GROUPS.map((g) => (
          <div key={g.label}>
            <div className="nav-group-label">{g.label}</div>
            {g.items.map((t) => (
              <button
                key={t.id}
                type="button"
                className={clsx("nav-btn", tab === t.id && "active")}
                onClick={() => setTab(t.id)}
              >
                {t.label}
              </button>
            ))}
          </div>
        ))}
      </aside>
      <div className="main">
        <header className="page-header">
          <span className={clsx("badge", engineState)}>{engineState.toUpperCase()}</span>
          {dryRun && <span className="badge dry">DRY RUN</span>}
          {Boolean((data?.heartbeat as Record<string, unknown>)?.kill) && (
            <span className="badge killed">KILL ON</span>
          )}
          <span className="page-header-meta">
            {viewActive ? slug : `No active ${viewAsset}/${viewTf} window`}
            {" · "}
            {viewAsset}/{viewTf} · snapshot 5s
          </span>
          {error && <span style={{ color: "var(--red)", fontSize: 13 }}>{error}</span>}
          {killMsg && <span style={{ color: "var(--amber)", fontSize: 13 }}>{killMsg}</span>}
        </header>

        {TRADING_TABS.has(tab) && (
          <ViewBar
            viewTf={viewTf}
            viewAsset={viewAsset}
            onViewTf={setViewTf}
            onViewAsset={setViewAsset}
            config={configBundle}
          />
        )}

        {(tab === "reports" || tab === "analytics") && (
          <ReportFiltersBar
            asset={reportAsset}
            tf={reportTf}
            result={reportResult}
            assetOptions={assetsList}
            onAsset={setReportAsset}
            onTf={setReportTf}
            onResult={setReportResult}
          />
        )}

        {tab === "overview" && (
          <Overview
            data={data}
            snap={snap}
            rawSnap={rawSnap}
            viewTf={viewTf}
            viewAsset={viewAsset}
            assetsList={assetsList}
            onSelectAsset={setViewAsset}
            onKill={onKill}
            onOpenPriceGraph={openCurrentPriceGraph}
            canOpenPriceGraph={viewActive && Boolean(slug)}
          />
        )}
        {tab === "live" && (
          <LiveMarket snap={snap} viewTf={viewTf} onOpenPriceGraph={openCurrentPriceGraph} />
        )}
        {tab === "chart" && (
          <PriceChart
            chartData={chartData}
            slug={seriesSlug}
            currentSlug={slug}
            viewingPast={viewingPastWindow}
            fillMarkers={fillMarkers}
            fillsLoadedForSlug={chartFills.length}
            onViewCurrent={openCurrentPriceGraph}
          />
        )}
        {tab === "positions" && <Positions orders={orders} fills={fills} snap={snap} />}
        {tab === "reports" && (
          <ReportsTable reports={reports} onSelect={(s) => { setSelectedSlug(s); setTab("chart"); }} />
        )}
        {tab === "analytics" && (
          <AnalyticsHub
            pnl={data?.pnl}
            reports={reports}
            hours={hours}
            daily={daily}
            buckets={buckets}
          />
        )}
        {tab === "risk" && <RiskPanel data={data} snap={snap} onKill={onKill} />}
        {tab === "ops" && <OpsPanel snap={snap} events={events} />}
      </div>
    </div>
  );
}

function ViewBar(props: {
  viewTf: string;
  viewAsset: string;
  onViewTf: (v: string) => void;
  onViewAsset: (v: string) => void;
  config?: Record<string, unknown>;
}) {
  const tfs = (props.config?.timeframes as string[] | undefined) ?? ["5m", "15m"];
  const assets = (props.config?.assets as string[] | undefined) ?? ["btc", "eth"];
  return (
    <div className="filters" style={{ flexWrap: "wrap", gap: 8 }}>
      <span className="metric-label" style={{ alignSelf: "center" }}>Asset</span>
      {assets.map((a) => (
        <button
          key={a}
          type="button"
          className={clsx("nav-btn", props.viewAsset === a && "active")}
          style={{ width: "auto", display: "inline-block", textTransform: "uppercase" }}
          onClick={() => props.onViewAsset(a)}
        >
          {a}
        </button>
      ))}
      <span className="metric-label" style={{ alignSelf: "center", marginLeft: 8 }}>Timeframe</span>
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
  assetOptions: string[];
  onAsset: (v: string) => void;
  onTf: (v: string) => void;
  onResult: (v: string) => void;
}) {
  return (
    <div className="filters">
      <span className="metric-label" style={{ alignSelf: "center" }}>Report filters</span>
      <select value={props.asset} onChange={(e) => props.onAsset(e.target.value)}>
        <option value="">All assets</option>
        {props.assetOptions.map((a) => (
          <option key={a} value={a}>{a.toUpperCase()}</option>
        ))}
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
  viewAsset,
  assetsList,
  onSelectAsset,
  onKill,
  onOpenPriceGraph,
  canOpenPriceGraph,
}: {
  data: DashboardPayload | null;
  snap: Record<string, unknown>;
  rawSnap: Record<string, unknown>;
  viewTf: string;
  viewAsset: string;
  assetsList: string[];
  onSelectAsset: (a: string) => void;
  onKill: () => void;
  onOpenPriceGraph: () => void;
  canOpenPriceGraph: boolean;
}) {
  const assetRows = assetWindowsForTf(rawSnap, viewTf, assetsList);
  const feeds = (snap.feeds ?? {}) as Record<string, unknown>;
  const spot = (feeds.chainlink_spot ?? {}) as Record<string, unknown>;
  const twap = (feeds.chainlink_twap ?? {}) as Record<string, unknown>;
  const coinbase = (feeds.coinbase_spot ?? {}) as Record<string, unknown>;
  const binance = (feeds.binance_spot ?? {}) as Record<string, unknown>;
  const pnl = data?.pnl;
  const totalPnl = parseFloat(String(pnl?.total_pnl ?? 0));

  return (
    <>
      <div className="overview-kpi">
        <div className="kpi-tile">
          <div className="kpi-label">Total PnL</div>
          <div className={clsx("kpi-value", pnlClass(totalPnl))}>{formatUsd(totalPnl)}</div>
        </div>
        <div className="kpi-tile">
          <div className="kpi-label">Win rate</div>
          <div className="kpi-value">{pct(pnl?.win_rate)}</div>
        </div>
        <div className="kpi-tile">
          <div className="kpi-label">Markets</div>
          <div className="kpi-value">{String(pnl?.trade_count ?? 0)}</div>
        </div>
        <div className="kpi-tile">
          <div className="kpi-label">Engine</div>
          <div className="kpi-value" style={{ fontSize: 18 }}>{String(snap.engine_state ?? "—")}</div>
        </div>
      </div>
      <div className="grid-2">
        <div className="card">
          <h3>Feeds</h3>
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
          <h3>Configured windows ({viewTf})</h3>
          <p className="metric-label">
            {assetsList.length} assets in config · selected {viewAsset.toUpperCase()}
          </p>
          <div className="asset-window-grid">
            {assetRows.map(({ asset, active, window }) => (
              <button
                key={asset}
                type="button"
                className={clsx(
                  "asset-window-card",
                  active && "active",
                  viewAsset === asset && "selected",
                )}
                onClick={() => onSelectAsset(asset)}
              >
                <div className="metric-label" style={{ textTransform: "uppercase" }}>{asset}</div>
                {active && window?.market ? (
                  <>
                    <MarketSummary market={window.market as Record<string, unknown>} />
                    <NextMarket next={window.next_market as Record<string, unknown>} />
                  </>
                ) : (
                  <p className="empty" style={{ margin: "8px 0 0" }}>Waiting for lifecycle…</p>
                )}
              </button>
            ))}
          </div>
          <div className="quick-links" style={{ marginTop: 12 }}>
            {canOpenPriceGraph && (
              <button type="button" className="btn-secondary" onClick={onOpenPriceGraph}>
                Price graph ({viewAsset})
              </button>
            )}
            <button type="button" className="btn-secondary" onClick={onKill}>
              Kill switch
            </button>
          </div>
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
      setResolved((prev) =>
        prev.tokenUp === tokenUp && prev.tokenDown === tokenDown ? prev : { tokenUp, tokenDown },
      );
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
  onOpenPriceGraph,
}: {
  snap: Record<string, unknown>;
  viewTf: string;
  onOpenPriceGraph: () => void;
}) {
  const viewActive = Boolean(snap.view_active);
  const market = viewActive ? (snap.current_market as Record<string, unknown> | undefined) : undefined;
  const strat = (snap.strategy ?? {}) as Record<string, unknown>;
  const signal = (snap.signal ?? {}) as Record<string, unknown>;
  const resolvedTokens = useMarketTokens(
    market?.slug as string | undefined,
    market?.token_up as string | undefined,
    market?.token_down as string | undefined,
  );
  const live = useLiveClobBooks(
    viewActive ? resolvedTokens.tokenUp : undefined,
    viewActive ? resolvedTokens.tokenDown : undefined,
  );
  const upFallback = (signal.up_bbo ?? {}) as Record<string, unknown>;
  const downFallback = (signal.down_bbo ?? {}) as Record<string, unknown>;
  const up = live.up.valid ? live.up : upFallback;
  const down = live.down.valid ? live.down : downFallback;

  if (!viewActive) {
    return <p className="empty">No active {viewTf} window.</p>;
  }

  return (
    <div className="grid-2">
      <div className="card">
        <h3>Market timing</h3>
        <MarketSummary market={market} />
        <p>Countdown to entry: {String(strat.secs_until_entry ?? "—")}s (after {String(strat.entry_after_sec ?? "—")}s)</p>
        <p>Strategy: <strong>{String(strat.state ?? "—")}</strong></p>
        <p>TWAP − beat: {String(strat.twap_minus_beat ?? "—")} (threshold {String(strat.twap_beat_diff_usd ?? "—")})</p>
        <p>Best ask &gt; min: {String(strat.best_ask_ok ?? "—")}</p>
        <p className="metric-label" style={{ marginTop: 8 }}>Last decision</p>
        <code className="decision-pill">{formatDecision(snap.last_decision)}</code>
        <button type="button" className="btn-secondary" style={{ marginTop: 12 }} onClick={onOpenPriceGraph}>
          View BTC price graph (this window)
        </button>
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

const LiveAskChart = memo(function LiveAskChart({
  data,
}: {
  data: { t: string; upAsk?: number; downAsk?: number }[];
}) {
  if (!data.length) {
    return <p className="empty">Waiting for CLOB ask updates…</p>;
  }
  return (
    <ResponsiveContainer width="100%" height={220} debounce={50}>
      <LineChart data={data}>
        <CartesianGrid stroke="#1e2836" strokeDasharray="3 3" />
        <XAxis dataKey="t" stroke="#8b9cb3" fontSize={11} minTickGap={28} />
        <YAxis stroke="#8b9cb3" fontSize={11} domain={[0, 1]} tickFormatter={(v) => Number(v).toFixed(2)} />
        <Tooltip
          contentStyle={{ background: "#121820", border: "1px solid #1e2836" }}
          formatter={(v) => (typeof v === "number" ? v.toFixed(4) : String(v ?? ""))}
        />
        <Legend />
        <Line type="monotone" dataKey="upAsk" name="Up ask" stroke="#f5a524" dot={false} strokeWidth={1.5} />
        <Line type="monotone" dataKey="downAsk" name="Down ask" stroke="#f04438" dot={false} strokeWidth={1.5} />
      </LineChart>
    </ResponsiveContainer>
  );
});

type ChartPoint = {
  t: string;
  xMs: number;
  tsIso?: string;
  beat?: number;
  spot?: number;
  twap?: number;
  coinbase?: number;
  binance?: number;
  coinbaseTwap?: number;
  binanceTwap?: number;
  coinbaseMom?: number;
  binanceMom?: number;
};

type FillMarker = { xMs: number; markY: number; label: string; t: string };

function PriceChart({
  chartData,
  slug,
  currentSlug,
  viewingPast,
  fillMarkers,
  fillsLoadedForSlug,
  onViewCurrent,
}: {
  chartData: ChartPoint[];
  slug: string;
  currentSlug: string;
  viewingPast: boolean;
  fillMarkers: FillMarker[];
  /** Raw fill rows for this slug (0 = none in DB for this window). */
  fillsLoadedForSlug: number;
  onViewCurrent: () => void;
}) {
  const hasCb = chartData.some((p) => p.coinbase != null);
  const hasBn = chartData.some((p) => p.binance != null);
  const hasCbTwap = chartData.some((p) => p.coinbaseTwap != null);
  const hasBnTwap = chartData.some((p) => p.binanceTwap != null);
  const hasMom =
    chartData.some((p) => p.coinbaseMom != null) || chartData.some((p) => p.binanceMom != null);
  const isLiveWindow = Boolean(currentSlug && slug === currentSlug);
  const [hoverIdx, setHoverIdx] = useState<number | null>(null);
  const onChartMouseMove = useCallback(
    (state: { activeTooltipIndex?: string | number | null }) => {
      setHoverIdx(chartPointIndex(state?.activeTooltipIndex, chartData));
    },
    [chartData],
  );
  const onChartMouseLeave = useCallback(() => setHoverIdx(null), []);
  const renderPriceTooltip = useCallback(
    (props: TooltipContentProps) => (
      <PriceGraphTooltip
        {...props}
        chartData={chartData}
        fillMarkers={fillMarkers}
        hoverIdx={hoverIdx}
      />
    ),
    [chartData, fillMarkers, hoverIdx],
  );
  const momYDomain = useMemo(() => momentumChartYDomain(chartData), [chartData]);
  const momTickFmt = useCallback(
    (v: number) => formatMomentumTick(v, momYDomain),
    [momYDomain],
  );
  const latestMom = chartData.length ? chartData[chartData.length - 1] : undefined;
  const hoverMomRow = hoverIdx != null ? chartData[hoverIdx] : latestMom;
  const renderMomTooltip = useCallback(
    (props: TooltipContentProps) => (
      <MomentumGraphTooltip {...props} chartData={chartData} hoverIdx={hoverIdx} momYDomain={momYDomain} />
    ),
    [chartData, hoverIdx, momYDomain],
  );
  return (
    <div className="card">
      <div style={{ display: "flex", flexWrap: "wrap", gap: 12, alignItems: "center", marginBottom: 8 }}>
        <h3 style={{ margin: 0 }}>
          {isLiveWindow ? "Live window" : "Past window"} · {slug || "—"}
        </h3>
        {viewingPast && currentSlug && (
          <button type="button" className="btn-secondary" onClick={onViewCurrent}>
            Back to current market ({currentSlug})
          </button>
        )}
        {!isLiveWindow && currentSlug && !viewingPast && slug && (
          <button type="button" className="btn-secondary" onClick={onViewCurrent}>
            Jump to current market
          </button>
        )}
      </div>
      <p className="metric-label" style={{ marginBottom: 12 }}>
        {chartAssetLabel(slug)} USD: Chainlink beat / spot / TWAP; Coinbase spot + local TWAP-60; momentum = % change over configured N sec (1 Hz samples, per asset).
        {fillMarkers.length > 0 && " Orange markers = fills (purchase time on CL spot)."}
        {" "}CLOB Up/Down asks are on Live market. Paper (dry-run) fills are stored the same way as live.
      </p>
      {chartData.length > 0 && fillsLoadedForSlug === 0 && (
        <p className="empty" style={{ marginBottom: 12 }}>
          No purchase recorded for this window — the strategy did not fill here (check Orders &amp; fills for other slugs).
        </p>
      )}
      {chartData.length === 0 ? (
        <p className="empty">No samples yet — engine writes 1Hz slot samples when a market is active.</p>
      ) : (
        <>
          <div className="chart-wrap">
            <ResponsiveContainer width="100%" height="100%">
              <LineChart
                data={chartData}
                onMouseMove={onChartMouseMove}
                onMouseLeave={onChartMouseLeave}
              >
                <CartesianGrid stroke="#1e2836" strokeDasharray="3 3" />
                <XAxis
                  dataKey="t"
                  type="category"
                  stroke="#8b9cb3"
                  fontSize={11}
                  interval="preserveStartEnd"
                  minTickGap={28}
                />
                <YAxis stroke="#8b9cb3" fontSize={11} domain={priceChartYDomain(chartData)} />
                <Tooltip
                  shared
                  cursor={{ stroke: "#8b9cb3", strokeWidth: 1 }}
                  content={renderPriceTooltip}
                />
                <Legend />
                <Line type="monotone" dataKey="beat" name="Beat" stroke="#7c5cff" dot={false} connectNulls={false} strokeWidth={2} />
                <Line type="monotone" dataKey="spot" name="CL spot" stroke="#4da3ff" dot={false} connectNulls={false} strokeWidth={1.5} />
                <Line type="monotone" dataKey="twap" name="CL TWAP" stroke="#3dd68c" dot={false} connectNulls={false} strokeWidth={1.5} />
                {hasCb && (
                  <Line type="monotone" dataKey="coinbase" name="CB spot" stroke="#f7931a" dot={false} connectNulls={false} strokeWidth={1.5} />
                )}
                {hasBn && (
                  <Line type="monotone" dataKey="binance" name="BN spot" stroke="#f0b90b" dot={false} connectNulls={false} strokeWidth={1.5} strokeDasharray="4 2" />
                )}
                {hasCbTwap && (
                  <Line type="monotone" dataKey="coinbaseTwap" name="CB TWAP60" stroke="#ffb347" dot={false} connectNulls={false} strokeWidth={1.25} strokeDasharray="2 2" />
                )}
                {hasBnTwap && (
                  <Line type="monotone" dataKey="binanceTwap" name="BN TWAP60" stroke="#ffe066" dot={false} connectNulls={false} strokeWidth={1.25} strokeDasharray="2 2" />
                )}
                {fillMarkers.map((m) => {
                  const anchor = nearestChartPointByMs(chartData, m.xMs);
                  if (!anchor) return null;
                  return (
                    <ReferenceDot
                      key={`fill-${m.xMs}-${m.label}`}
                      x={anchor.t}
                      y={m.markY}
                      r={7}
                      fill="#f5a524"
                      stroke="#1a1208"
                      strokeWidth={2}
                      label={{
                        value: m.label,
                        position: "top",
                        fill: "#f5a524",
                        fontSize: 11,
                      }}
                    />
                  );
                })}
              </LineChart>
            </ResponsiveContainer>
          </div>
          {hasMom && (
            <div style={{ marginTop: 20 }}>
              <div
                style={{
                  display: "flex",
                  flexWrap: "wrap",
                  gap: 16,
                  alignItems: "baseline",
                  marginBottom: 10,
                }}
              >
                <h4 style={{ margin: 0, fontSize: 14, color: "#c5d0de" }}>Short-term momentum</h4>
                {hoverMomRow?.coinbaseMom != null && (
                  <span className="metric-label" style={{ color: momentumColor(hoverMomRow.coinbaseMom) }}>
                    CB{" "}
                    <strong style={{ fontFamily: "var(--font-mono, monospace)" }}>
                      {formatMomentumPct(hoverMomRow.coinbaseMom, momYDomain)}
                    </strong>
                    {hoverIdx != null ? " @ cursor" : " · latest"}
                  </span>
                )}
                {hoverMomRow?.binanceMom != null && (
                  <span className="metric-label" style={{ color: momentumColor(hoverMomRow.binanceMom) }}>
                    BN{" "}
                    <strong style={{ fontFamily: "var(--font-mono, monospace)" }}>
                      {formatMomentumPct(hoverMomRow.binanceMom, momYDomain)}
                    </strong>
                  </span>
                )}
                <span className="metric-label" style={{ opacity: 0.75 }}>
                  Y scale auto ({formatMomentumPct(momYDomain[0], momYDomain)} … {formatMomentumPct(momYDomain[1], momYDomain)})
                </span>
              </div>
              <div className="chart-wrap chart-wrap-momentum" style={{ height: 200 }}>
                <ResponsiveContainer width="100%" height="100%">
                  <LineChart
                    data={chartData}
                    onMouseMove={onChartMouseMove}
                    onMouseLeave={onChartMouseLeave}
                    margin={{ top: 8, right: 12, left: 4, bottom: 0 }}
                  >
                    <defs>
                      <linearGradient id="cbMomArea" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="0%" stopColor="#3dd68c" stopOpacity={0.45} />
                        <stop offset="45%" stopColor="#f7931a" stopOpacity={0.12} />
                        <stop offset="100%" stopColor="#ff6b6b" stopOpacity={0.4} />
                      </linearGradient>
                      <linearGradient id="bnMomArea" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="0%" stopColor="#ffe066" stopOpacity={0.35} />
                        <stop offset="100%" stopColor="#f0b90b" stopOpacity={0.08} />
                      </linearGradient>
                    </defs>
                    <CartesianGrid stroke="#1e2836" strokeDasharray="3 3" vertical={false} />
                    <XAxis
                      dataKey="t"
                      type="category"
                      stroke="#8b9cb3"
                      fontSize={11}
                      interval="preserveStartEnd"
                      minTickGap={28}
                      tickLine={false}
                    />
                    <YAxis
                      stroke="#8b9cb3"
                      fontSize={11}
                      domain={momYDomain}
                      tickFormatter={momTickFmt}
                      width={56}
                      tickLine={false}
                    />
                    <Tooltip cursor={{ stroke: "#8b9cb3", strokeWidth: 1 }} content={renderMomTooltip} />
                    <Legend wrapperStyle={{ fontSize: 12, paddingTop: 4 }} />
                    <ReferenceLine y={0} stroke="#5a6a82" strokeWidth={1.5} />
                    {hoverIdx != null && chartData[hoverIdx]?.t && (
                      <ReferenceLine x={chartData[hoverIdx].t} stroke="#8b9cb3" strokeDasharray="4 4" />
                    )}
                    {chartData.some((p) => p.coinbaseMom != null) && (
                      <Area
                        type="monotone"
                        dataKey="coinbaseMom"
                        name="CB momentum"
                        stroke="none"
                        fill="url(#cbMomArea)"
                        connectNulls={false}
                        isAnimationActive={false}
                      />
                    )}
                    {chartData.some((p) => p.binanceMom != null) && (
                      <Area
                        type="monotone"
                        dataKey="binanceMom"
                        name="BN momentum"
                        stroke="none"
                        fill="url(#bnMomArea)"
                        connectNulls={false}
                        isAnimationActive={false}
                      />
                    )}
                    {chartData.some((p) => p.coinbaseMom != null) && (
                      <Line
                        type="monotone"
                        dataKey="coinbaseMom"
                        name="CB momentum"
                        stroke="#f7931a"
                        dot={false}
                        connectNulls={false}
                        strokeWidth={2}
                        activeDot={{ r: 5, stroke: "#1a1208", strokeWidth: 2, fill: "#f7931a" }}
                        legendType="none"
                        isAnimationActive={false}
                      />
                    )}
                    {chartData.some((p) => p.binanceMom != null) && (
                      <Line
                        type="monotone"
                        dataKey="binanceMom"
                        name="BN momentum"
                        stroke="#f0b90b"
                        dot={false}
                        connectNulls={false}
                        strokeWidth={2}
                        strokeDasharray="6 3"
                        activeDot={{ r: 5, stroke: "#1a1208", strokeWidth: 2, fill: "#f0b90b" }}
                        legendType="none"
                        isAnimationActive={false}
                      />
                    )}
                  </LineChart>
                </ResponsiveContainer>
              </div>
            </div>
          )}
          {fillMarkers.length > 0 && (
            <ul className="metric-label" style={{ marginTop: 12, paddingLeft: 18 }}>
              {fillMarkers.map((m) => (
                <li key={`${m.xMs}-${m.label}`}>
                  Purchase {m.t} — {m.label} (on CL spot line)
                </li>
              ))}
            </ul>
          )}
        </>
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
        <h3>Orders</h3>
        {orders.length === 0 ? <p className="empty">No orders yet — they appear here when the strategy fires.</p> : (
          <DataTable
            columns={["slug", "side", "signed_px", "size", "order_type", "status"]}
            rows={orders}
          />
        )}
      </div>
      <div className="card">
        <h3>Fills</h3>
        {fills.length === 0 ? <p className="empty">No fills yet.</p> : (
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
      <h3 style={{ textTransform: "none", fontSize: 15, color: "var(--text)" }}>Closed windows</h3>
      <p className="metric-label" style={{ marginTop: -8, marginBottom: 16 }}>Click a row to open its price graph.</p>
      {reports.length === 0 ? (
        <p className="empty">No closed market reports yet.</p>
      ) : (
        <table className="data">
          <thead>
            <tr>
              <th>Slug</th>
              <th>Result</th>
              <th>Paper PnL</th>
              <th>Paper net</th>
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

function formatDecision(raw: unknown): string {
  if (raw == null) return "—";
  if (typeof raw === "string") return raw;
  try {
    const o = raw as Record<string, unknown>;
    if (o.decision != null) return String(o.decision);
    return JSON.stringify(raw);
  } catch {
    return String(raw);
  }
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

function clockLabelFromTs(raw: unknown): string {
  if (raw == null) return "";
  if (typeof raw === "string") {
    if (raw.includes("T")) return raw.slice(11, 19);
    return raw.length >= 19 ? raw.slice(11, 19) : raw.slice(0, 8);
  }
  const ms = Date.parse(String(raw));
  if (Number.isFinite(ms)) return new Date(ms).toISOString().slice(11, 19);
  return "";
}

function tsToMs(raw: unknown): number {
  if (raw == null) return NaN;
  if (typeof raw === "number") return raw;
  return Date.parse(String(raw));
}

function formatChartTickMs(ms: number): string {
  if (!Number.isFinite(ms)) return "";
  return new Date(ms).toISOString().slice(11, 19);
}

function dedupeChartBySecond(rows: ChartPoint[]): ChartPoint[] {
  const bySecond = new Map<string, ChartPoint>();
  for (const r of rows) {
    if (!r.t) continue;
    bySecond.set(r.t, r);
  }
  return [...bySecond.values()].sort((a, b) => a.xMs - b.xMs);
}

/** Drop window-open garbage (wrong beat / cross-asset ticks) that causes vertical Recharts spikes. */
function sanitizeChartSeries(rows: ChartPoint[], slug: string): ChartPoint[] {
  if (rows.length === 0) return rows;
  let sorted = [...rows].sort((a, b) => a.xMs - b.xMs);

  const openMs = windowOpenMsFromSlug(slug);
  if (openMs != null) {
    const warmupEnd = openMs + 3000;
    sorted = sorted.filter((p) => p.xMs >= warmupEnd);
  }

  const ref = robustChartAnchor(sorted);
  if (ref == null) return sorted;

  const inBand = (v?: number) =>
    v != null && Number.isFinite(v) && v > 0 && v >= ref * 0.5 && v <= ref * 2;

  let cleaned = sorted.map((p) => ({
    ...p,
    beat: inBand(p.beat) ? p.beat : undefined,
    spot: inBand(p.spot) ? p.spot : undefined,
    twap: inBand(p.twap) ? p.twap : undefined,
    coinbase: inBand(p.coinbase) ? p.coinbase : undefined,
    binance: inBand(p.binance) ? p.binance : undefined,
    coinbaseTwap: inBand(p.coinbaseTwap) ? p.coinbaseTwap : undefined,
    binanceTwap: inBand(p.binanceTwap) ? p.binanceTwap : undefined,
  }));

  while (cleaned.length > 1) {
    const head = cleaned[0];
    const hasFeed = inBand(head.spot) || inBand(head.twap) || inBand(head.coinbase) || inBand(head.beat);
    if (hasFeed) break;
    cleaned = cleaned.slice(1);
  }

  while (cleaned.length > 1 && headIsChartOutlier(cleaned[0], cleaned[1], ref)) {
    cleaned = cleaned.slice(1);
  }

  return cleaned;
}

function windowOpenMsFromSlug(slug: string): number | null {
  if (!slug) return null;
  const tail = slug.split("-").pop() ?? "";
  const openSec = parseInt(tail, 10);
  if (!Number.isFinite(openSec) || openSec <= 0) return null;
  return openSec * 1000;
}

function windowDurationMsFromSlug(slug: string): number {
  const m = slug.toLowerCase().match(/-updown-(\d+)m-/);
  if (m) return parseInt(m[1], 10) * 60 * 1000;
  const h = slug.toLowerCase().match(/-updown-(\d+)h-/);
  if (h) return parseInt(h[1], 10) * 3600 * 1000;
  return 5 * 60 * 1000;
}

/** Extend category X-axis to window open/close even when samples start late or stop early. */
function padChartToWindow(rows: ChartPoint[], slug: string): ChartPoint[] {
  const openMs = windowOpenMsFromSlug(slug);
  if (openMs == null || rows.length === 0) return rows;
  const closeMs = openMs + windowDurationMsFromSlug(slug);
  const sorted = [...rows].sort((a, b) => a.xMs - b.xMs);
  const out = [...sorted];
  const pad = (ms: number): ChartPoint => ({
    t: new Date(ms).toISOString().slice(11, 19),
    xMs: ms,
  });
  if (sorted[0].xMs > openMs + 800) out.unshift(pad(openMs));
  const last = sorted[sorted.length - 1];
  if (last.xMs < closeMs - 800) out.push(pad(closeMs));
  return out.sort((a, b) => a.xMs - b.xMs);
}

function robustChartAnchor(rows: ChartPoint[]): number | null {
  const vals: number[] = [];
  for (const p of rows) {
    for (const v of [p.spot, p.twap, p.coinbase, p.coinbaseTwap, p.binance, p.binanceTwap]) {
      if (v != null && v > 0 && v < 10_000_000) vals.push(v);
    }
  }
  if (vals.length === 0) return null;
  vals.sort((a, b) => a - b);
  return vals[Math.floor(vals.length / 2)];
}

/** Beat is window-open TWAP — must be same order of magnitude as spot/TWAP (not another asset's beat). */
function beatMatchesPriceScale(beat: number | undefined, anchor: number | null): number | undefined {
  if (beat == null || anchor == null || anchor <= 0) return undefined;
  const ratio = beat / anchor;
  if (ratio < 0.85 || ratio > 1.15) return undefined;
  return beat;
}

function resolveWindowBeat(
  series: Record<string, unknown>[],
  seriesSlug: string,
  reports: Record<string, unknown>[],
  currentMarket: Record<string, unknown> | undefined,
  anchor: number | null,
): number | undefined {
  if (anchor == null) return undefined;

  const tryBeat = (raw?: string) =>
    beatMatchesPriceScale(chartPrice(raw ?? ""), anchor);

  for (const p of series) {
    const pt = (p.point ?? {}) as Record<string, string>;
    const b = tryBeat(pt.beat);
    if (b != null) return b;
  }

  const fromReport = reports.find((r) => String(r.slug) === seriesSlug);
  const reportBeat = tryBeat(String(fromReport?.beat ?? ""));
  if (reportBeat != null) return reportBeat;

  if (String(currentMarket?.slug) === seriesSlug) {
    const liveBeat = tryBeat(String(currentMarket?.beat ?? ""));
    if (liveBeat != null) return liveBeat;
  }

  return undefined;
}

function headIsChartOutlier(a: ChartPoint, b: ChartPoint, ref: number): boolean {
  const pick = (p: ChartPoint) => p.beat ?? p.spot ?? p.twap ?? p.coinbase;
  const va = pick(a);
  const vb = pick(b);
  if (va == null) return true;
  if (vb == null) return va < ref * 0.5 || va > ref * 2;
  return Math.abs(va - vb) / vb > 0.05 && (va < ref * 0.5 || va > ref * 2);
}

function chartPointIndex(
  activeIndex: string | number | null | undefined,
  chartData: ChartPoint[],
): number | null {
  if (activeIndex == null || !chartData.length) return null;
  if (typeof activeIndex === "number" && activeIndex >= 0 && activeIndex < chartData.length) {
    return activeIndex;
  }
  const s = String(activeIndex);
  const asNum = Number(s);
  if (Number.isInteger(asNum) && asNum >= 0 && asNum < chartData.length) return asNum;
  const byKey = chartData.findIndex((p) => p.t === s || String(p.xMs) === s);
  return byKey >= 0 ? byKey : null;
}

function PriceGraphTooltip({
  active,
  activeIndex,
  label,
  chartData,
  fillMarkers,
  hoverIdx,
}: TooltipContentProps & {
  chartData: ChartPoint[];
  fillMarkers: FillMarker[];
  hoverIdx: number | null;
}) {
  if (!active || !chartData.length) return null;
  const idx =
    hoverIdx ??
    chartPointIndex(activeIndex, chartData) ??
    chartPointIndex(label, chartData);
  const row = idx != null ? chartData[idx] : undefined;
  if (!row) return null;
  const xMs = row.xMs;
  const purchase = fillMarkers.find((m) => Math.abs(m.xMs - xMs) < 1500);

  const rows: { name: string; value?: number; color: string }[] = [
    { name: "Beat", value: row.beat, color: "#7c5cff" },
    { name: "CL spot", value: row.spot, color: "#4da3ff" },
    { name: "CL TWAP", value: row.twap, color: "#3dd68c" },
    { name: "CB spot", value: row.coinbase, color: "#f7931a" },
    { name: "CB TWAP60", value: row.coinbaseTwap, color: "#ffb347" },
    { name: "BN spot", value: row.binance, color: "#f0b90b" },
    { name: "BN TWAP60", value: row.binanceTwap, color: "#ffe066" },
  ];

  return (
    <div
      style={{
        background: "#121820",
        border: "1px solid #1e2836",
        padding: "10px 12px",
        fontSize: 12,
        borderRadius: 6,
      }}
    >
      <div style={{ color: "#8b9cb3", marginBottom: 8 }}>{formatChartTickMs(Number(xMs))}</div>
      {rows.map(
        (r) =>
          r.value != null && (
            <div key={r.name} style={{ color: r.color, marginTop: 4 }}>
              {r.name} : {formatChartPrice(r.value)}
            </div>
          ),
      )}
      {purchase && (
        <div style={{ color: "#f5a524", marginTop: 4 }}>
          Purchase : {purchase.label} ({formatChartPrice(purchase.markY)} on CL spot)
        </div>
      )}
    </div>
  );
}

function formatChartPrice(n: number) {
  if (!Number.isFinite(n)) return "—";
  if (n >= 1000) return n.toFixed(2);
  return n.toPrecision(6);
}

function nearestChartPointByMs(chartData: ChartPoint[], xMs: number): ChartPoint | undefined {
  if (!chartData.length || !Number.isFinite(xMs)) return undefined;
  let best = chartData[0];
  let bestDiff = Math.abs(best.xMs - xMs);
  for (const p of chartData) {
    const d = Math.abs(p.xMs - xMs);
    if (d < bestDiff) {
      best = p;
      bestDiff = d;
    }
  }
  return best;
}

function buildFillMarkers(chartData: ChartPoint[], fills: Record<string, unknown>[]): FillMarker[] {
  if (!chartData.length || !fills.length) return [];
  return fills
    .map((f) => {
      const xMs = tsToMs(f.ts);
      if (!Number.isFinite(xMs)) return null;
      const clock = clockLabelFromTs(f.ts);
      const pt =
        chartData.find((p) => p.t === clock) ??
        chartData.find((p) => Math.abs(p.xMs - xMs) < 1500) ??
        nearestChartPointByMs(chartData, xMs);
      const markY = pt?.spot ?? pt?.twap ?? pt?.beat ?? pt?.coinbase;
      if (markY == null) return null;
      const side = String(f.side ?? "?");
      const px = String(f.px ?? "—");
      return {
        xMs,
        markY,
        t: clock || formatChartTickMs(xMs),
        label: `${side} @ ${px}`,
      };
    })
    .filter((m): m is FillMarker => m != null);
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

/** Momentum series: stored as percent (e.g. 0.15 = 0.15%). */
function chartPct(v?: string) {
  if (v == null || v === "" || v === "null") return undefined;
  const n = parseFloat(v);
  if (!Number.isFinite(n) || Math.abs(n) > 50) return undefined;
  return n;
}

/** USD price series: omit invalid / absurd magnitudes (bad first ticks at window open). */
function chartPrice(v?: string) {
  if (v == null || v === "" || v === "null") return undefined;
  const n = parseFloat(v);
  if (!Number.isFinite(n) || n <= 0 || n >= 10_000_000) return undefined;
  return n;
}

function chartAssetLabel(slug: string) {
  const a = slug.split("-")[0]?.toUpperCase();
  return a || "Asset";
}

function momentumChartYDomain(data: ChartPoint[]): [number, number] {
  const vals: number[] = [];
  for (const p of data) {
    if (p.coinbaseMom != null && Number.isFinite(p.coinbaseMom)) vals.push(p.coinbaseMom);
    if (p.binanceMom != null && Number.isFinite(p.binanceMom)) vals.push(p.binanceMom);
  }
  if (vals.length === 0) return [-0.05, 0.05];
  let min = Math.min(...vals);
  let max = Math.max(...vals);
  min = Math.min(min, 0);
  max = Math.max(max, 0);
  const span = Math.max(max - min, 0.0005);
  const pad = Math.max(span * 0.2, 0.0015);
  return [min - pad, max + pad];
}

function formatMomentumTick(v: number, domain: [number, number]): string {
  const span = domain[1] - domain[0];
  const decimals = span < 0.02 ? 4 : span < 0.2 ? 3 : 2;
  return `${v.toFixed(decimals)}%`;
}

function formatMomentumPct(v: number, domain?: [number, number]): string {
  const span = domain ? domain[1] - domain[0] : Math.abs(v) * 2 || 0.01;
  const decimals = span < 0.02 ? 4 : span < 0.2 ? 3 : 2;
  const sign = v > 0 ? "+" : "";
  return `${sign}${v.toFixed(decimals)}%`;
}

function momentumColor(v: number): string {
  if (v > 0.0005) return "#3dd68c";
  if (v < -0.0005) return "#ff6b6b";
  return "#f7931a";
}

function MomentumGraphTooltip({
  active,
  activeIndex,
  label,
  chartData,
  hoverIdx,
  momYDomain,
}: TooltipContentProps & {
  chartData: ChartPoint[];
  hoverIdx: number | null;
  momYDomain: [number, number];
}) {
  if (!active || !chartData.length) return null;
  const idx =
    hoverIdx ?? chartPointIndex(activeIndex, chartData) ?? chartPointIndex(label, chartData);
  const row = idx != null ? chartData[idx] : undefined;
  if (!row) return null;
  const series: { name: string; value?: number; color: string }[] = [
    { name: "CB momentum", value: row.coinbaseMom, color: "#f7931a" },
    { name: "BN momentum", value: row.binanceMom, color: "#f0b90b" },
  ];
  return (
    <div
      style={{
        background: "#121820",
        border: "1px solid #1e2836",
        padding: "10px 12px",
        fontSize: 12,
        borderRadius: 6,
        boxShadow: "0 8px 24px rgba(0,0,0,0.35)",
      }}
    >
      <div style={{ color: "#8b9cb3", marginBottom: 8 }}>{formatChartTickMs(row.xMs)}</div>
      {series.map(
        (s) =>
          s.value != null && (
            <div key={s.name} style={{ color: momentumColor(s.value), marginTop: 4 }}>
              <span style={{ color: s.color }}>{s.name}</span> :{" "}
              {formatMomentumPct(s.value, momYDomain)}
            </div>
          ),
      )}
    </div>
  );
}

function priceChartYDomain(data: ChartPoint[]): [number, number] | ["auto", "auto"] {
  const vals: number[] = [];
  for (const p of data) {
    for (const k of ["beat", "spot", "twap", "coinbase", "binance", "coinbaseTwap", "binanceTwap"] as const) {
      const v = p[k];
      if (v != null && Number.isFinite(v)) vals.push(v);
    }
  }
  if (vals.length === 0) return ["auto", "auto"];
  const min = Math.min(...vals);
  const max = Math.max(...vals);
  const pad = Math.max((max - min) * 0.08, min * 0.0005, 0.01);
  return [min - pad, max + pad];
}

function formatAge(age: unknown) {
  if (age == null || age === "") return "no data";
  const n = Number(age);
  if (!Number.isFinite(n) || n < 0) return "no data";
  return `${Math.round(n)}ms`;
}

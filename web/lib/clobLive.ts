"use client";

import { useEffect, useRef, useState } from "react";

export type LiveBbo = {
  bid: string;
  ask: string;
  bid_sz: string;
  ask_sz: string;
  spread: string;
  valid: boolean;
  live: boolean;
};

const CLOB_MARKET_WS =
  process.env.NEXT_PUBLIC_CLOB_WS ?? "wss://ws-subscriptions-clob.polymarket.com/ws/market";

export type AskPoint = {
  t: string;
  upAsk?: number;
  downAsk?: number;
};

type SideBooks = {
  up: LiveBbo;
  down: LiveBbo;
  connected: boolean;
  askSeries: AskPoint[];
};

const MAX_ASK_POINTS = 360;
const ASK_SAMPLE_MS = 250;

const emptyBbo = (): LiveBbo => ({
  bid: "0",
  ask: "0",
  bid_sz: "0",
  ask_sz: "0",
  spread: "0",
  valid: false,
  live: false,
});

function decSpread(bid: number, ask: number): string {
  if (!bid && !ask) return "0";
  return (ask - bid).toFixed(4).replace(/\.?0+$/, "") || "0";
}

function parseNum(v: unknown): number {
  if (typeof v === "string") return parseFloat(v) || 0;
  if (typeof v === "number") return v;
  return 0;
}

function bboFromLevels(bids: unknown[], asks: unknown[]): LiveBbo | null {
  let bid = 0;
  let bidSz = 0;
  for (const level of bids) {
    const row = level as Record<string, unknown>;
    const px = parseNum(row.price);
    if (px > bid) {
      bid = px;
      bidSz = parseNum(row.size);
    }
  }
  let ask = 0;
  let askSz = 0;
  for (const level of asks) {
    const row = level as Record<string, unknown>;
    const px = parseNum(row.price);
    if (px > 0 && (ask === 0 || px < ask)) {
      ask = px;
      askSz = parseNum(row.size);
    }
  }
  if (!bid && !ask) return null;
  return {
    bid: String(bid),
    ask: String(ask),
    bid_sz: String(bidSz),
    ask_sz: String(askSz),
    spread: decSpread(bid, ask),
    valid: true,
    live: true,
  };
}

function applyEvent(
  v: Record<string, unknown>,
  tokenUp: string,
  tokenDown: string,
  cache: Map<string, LiveBbo>,
): SideBooks | null {
  const eventType = (v.event_type as string) ?? (v.type as string) ?? "";
  const payload = (v.payload as Record<string, unknown>) ?? v;

  if (eventType === "book") {
    const asset = String(payload.asset_id ?? payload.assetId ?? "");
    const next = bboFromLevels(
      (payload.bids as unknown[]) ?? [],
      (payload.asks as unknown[]) ?? [],
    );
    if (asset && next) cache.set(asset, next);
  } else if (eventType === "best_bid_ask") {
    const asset = String(payload.asset_id ?? payload.assetId ?? "");
    const bid = parseNum(payload.best_bid ?? payload.bestBid);
    const ask = parseNum(payload.best_ask ?? payload.bestAsk);
    if (asset && (bid || ask)) {
      cache.set(asset, {
        bid: String(bid),
        ask: String(ask),
        bid_sz: String(parseNum(payload.best_bid_size ?? payload.bestBidSize)),
        ask_sz: String(parseNum(payload.best_ask_size ?? payload.bestAskSize)),
        spread: decSpread(bid, ask),
        valid: true,
        live: true,
      });
    }
  } else if (eventType === "price_change") {
    const changes =
      (payload.price_changes as Record<string, unknown>[]) ??
      (payload.priceChanges as Record<string, unknown>[]) ??
      [];
    for (const ch of changes) {
      const asset = String(ch.asset_id ?? ch.assetId ?? "");
      if (!asset) continue;
      const prev = cache.get(asset) ?? emptyBbo();
      const bid = ch.best_bid ?? ch.bestBid;
      const ask = ch.best_ask ?? ch.bestAsk;
      const next: LiveBbo = {
        bid: bid != null ? String(parseNum(bid)) : prev.bid,
        ask: ask != null ? String(parseNum(ask)) : prev.ask,
        bid_sz: prev.bid_sz,
        ask_sz: prev.ask_sz,
        spread: decSpread(
          parseNum(bid != null ? bid : prev.bid),
          parseNum(ask != null ? ask : prev.ask),
        ),
        valid: true,
        live: true,
      };
      cache.set(asset, next);
    }
  }

  return {
    up: cache.get(tokenUp) ?? emptyBbo(),
    down: cache.get(tokenDown) ?? emptyBbo(),
    connected: true,
  };
}

async function seedRest(tokenId: string, cache: Map<string, LiveBbo>) {
  try {
    const r = await fetch(`https://clob.polymarket.com/book?token_id=${encodeURIComponent(tokenId)}`, {
      cache: "no-store",
    });
    if (!r.ok) return;
    const v = (await r.json()) as Record<string, unknown>;
    const bbo = bboFromLevels((v.bids as unknown[]) ?? [], (v.asks as unknown[]) ?? []);
    if (bbo) cache.set(tokenId, bbo);
  } catch {
    /* ignore */
  }
}

function timeLabel(d = new Date()): string {
  return d.toISOString().slice(11, 19);
}

export function useLiveClobBooks(tokenUp?: string, tokenDown?: string): SideBooks {
  const [state, setState] = useState<SideBooks>({
    up: emptyBbo(),
    down: emptyBbo(),
    connected: false,
    askSeries: [],
  });
  const cacheRef = useRef(new Map<string, LiveBbo>());
  const askSeriesRef = useRef<AskPoint[]>([]);
  const lastAskSampleMsRef = useRef(0);

  useEffect(() => {
    if (!tokenUp || !tokenDown) {
      askSeriesRef.current = [];
      lastAskSampleMsRef.current = 0;
      setState({ up: emptyBbo(), down: emptyBbo(), connected: false, askSeries: [] });
      return;
    }

    const cache = cacheRef.current;
    cache.clear();
    askSeriesRef.current = [];
    lastAskSampleMsRef.current = 0;
    let closed = false;
    let ws: WebSocket | null = null;
    let pingTimer: ReturnType<typeof setInterval> | null = null;

    const maybeAppendAsk = (up: LiveBbo, down: LiveBbo, force = false) => {
      const upAsk = parseNum(up.ask);
      const downAsk = parseNum(down.ask);
      if (upAsk <= 0 && downAsk <= 0) return askSeriesRef.current;

      const now = Date.now();
      if (!force && now - lastAskSampleMsRef.current < ASK_SAMPLE_MS) {
        return askSeriesRef.current;
      }
      lastAskSampleMsRef.current = now;

      const point: AskPoint = {
        t: timeLabel(new Date(now)),
        ...(upAsk > 0 ? { upAsk } : {}),
        ...(downAsk > 0 ? { downAsk } : {}),
      };
      const next = [...askSeriesRef.current, point];
      if (next.length > MAX_ASK_POINTS) next.splice(0, next.length - MAX_ASK_POINTS);
      askSeriesRef.current = next;
      return next;
    };

    const publish = (forceAsk = false) => {
      const up = cache.get(tokenUp) ?? emptyBbo();
      const down = cache.get(tokenDown) ?? emptyBbo();
      const askSeries = maybeAppendAsk(up, down, forceAsk);
      setState({
        up,
        down,
        connected: ws?.readyState === WebSocket.OPEN,
        askSeries,
      });
    };

    void (async () => {
      await Promise.all([seedRest(tokenUp, cache), seedRest(tokenDown, cache)]);
      if (!closed) publish(true);
    })();

    ws = new WebSocket(CLOB_MARKET_WS);
    ws.onopen = () => {
      ws?.send(
        JSON.stringify({
          type: "market",
          assets_ids: [tokenUp, tokenDown],
          custom_feature_enabled: true,
        }),
      );
      publish();
      pingTimer = setInterval(() => {
        if (ws?.readyState === WebSocket.OPEN) ws.send("PING");
      }, 10_000);
    };
    ws.onmessage = (ev) => {
      const text = String(ev.data);
      if (text === "PONG" || text === "PING") return;
      try {
        const parsed = JSON.parse(text) as unknown;
        const list = Array.isArray(parsed) ? parsed : [parsed];
        for (const item of list) {
          if (item && typeof item === "object") {
            const next = applyEvent(item as Record<string, unknown>, tokenUp, tokenDown, cache);
            if (next) {
              const askSeries = maybeAppendAsk(next.up, next.down);
              setState({ ...next, connected: true, askSeries });
            }
          }
        }
      } catch {
        /* ignore */
      }
    };
    ws.onclose = () => {
      if (!closed) setState((s) => ({ ...s, connected: false }));
    };

    return () => {
      closed = true;
      if (pingTimer) clearInterval(pingTimer);
      ws?.close();
    };
  }, [tokenUp, tokenDown]);

  return state;
}

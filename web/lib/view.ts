/** Per-market window payload from the engine snapshot. */
export type WindowView = {
  active?: boolean;
  timeframe?: string;
  market?: Record<string, unknown>;
  next_market?: Record<string, unknown>;
  signal?: Record<string, unknown>;
  strategy?: Record<string, unknown>;
  last_decision?: Record<string, unknown>;
};

export type TimeframeWindow = WindowView & {
  markets?: WindowView[];
};

/** Configured assets from snapshot or dashboard config_meta. */
export function configuredAssets(
  snap: Record<string, unknown>,
  configMeta?: Record<string, unknown> | null,
): string[] {
  const fromSnap = (snap.config as Record<string, unknown> | undefined)?.assets;
  if (Array.isArray(fromSnap) && fromSnap.length > 0) {
    return fromSnap as string[];
  }
  const fromMeta = configMeta?.assets;
  if (Array.isArray(fromMeta) && fromMeta.length > 0) {
    return fromMeta as string[];
  }
  return ["btc", "eth"];
}

/** Pick engine window payload for the selected timeframe (5m / 15m). */
export function pickWindow(
  snap: Record<string, unknown>,
  viewTf: string,
): TimeframeWindow | null {
  const windows = snap.windows as Record<string, TimeframeWindow> | undefined;
  if (windows?.[viewTf]?.active) {
    return windows[viewTf];
  }
  if (viewTf !== "5m" && windows?.["5m"]?.active) {
    return windows["5m"];
  }
  return null;
}

/** All per-asset window views for a timeframe (includes placeholders for inactive assets). */
export function assetWindowsForTf(
  snap: Record<string, unknown>,
  viewTf: string,
  assets: string[],
): { asset: string; active: boolean; window: WindowView | null }[] {
  const tfWin = pickWindow(snap, viewTf);
  const byAsset = new Map<string, WindowView>();
  if (tfWin?.markets?.length) {
    for (const w of tfWin.markets) {
      const a = String((w.market as Record<string, unknown> | undefined)?.asset ?? "");
      if (a) byAsset.set(a, w);
    }
  } else if (tfWin?.market) {
    const a = String((tfWin.market as Record<string, unknown>).asset ?? "");
    if (a) byAsset.set(a, tfWin);
  }
  return assets.map((asset) => {
    const window = byAsset.get(asset) ?? null;
    return { asset, active: window != null, window };
  });
}

function pickAssetWindow(tfWin: TimeframeWindow | null, viewAsset: string): WindowView | null {
  if (!tfWin?.active) return null;
  if (tfWin.markets?.length) {
    const found = tfWin.markets.find(
      (w) => (w.market as Record<string, unknown> | undefined)?.asset === viewAsset,
    );
    return found ?? tfWin.markets[0] ?? null;
  }
  if (tfWin.market) {
    const asset = (tfWin.market as Record<string, unknown>).asset;
    if (!viewAsset || asset === viewAsset) return tfWin;
    return tfWin;
  }
  return null;
}

/** Merge window view into snapshot shape expected by overview/live components. */
export function viewSnapshot(
  snap: Record<string, unknown>,
  viewTf: string,
  viewAsset: string,
): Record<string, unknown> {
  const tfWin = pickWindow(snap, viewTf);
  const selected = pickAssetWindow(tfWin, viewAsset);
  if (!selected?.market) {
    return {
      ...snap,
      current_market: undefined,
      signal: undefined,
      strategy: undefined,
      next_market: undefined,
      last_decision: undefined,
      view_timeframe: viewTf,
      view_asset: viewAsset,
      view_active: false,
    };
  }
  return {
    ...snap,
    current_market: selected.market,
    signal: selected.signal,
    strategy: selected.strategy,
    next_market: selected.next_market,
    last_decision: selected.last_decision,
    view_timeframe: viewTf,
    view_asset: viewAsset,
    view_active: true,
  };
}

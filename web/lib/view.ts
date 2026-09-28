/** Pick engine window payload for the selected timeframe (5m / 15m). */
export type WindowView = {
  active?: boolean;
  timeframe?: string;
  market?: Record<string, unknown>;
  next_market?: Record<string, unknown>;
  signal?: Record<string, unknown>;
  strategy?: Record<string, unknown>;
  last_decision?: Record<string, unknown>;
};

export function pickWindow(
  snap: Record<string, unknown>,
  viewTf: string,
): WindowView | null {
  const windows = snap.windows as Record<string, WindowView> | undefined;
  if (windows?.[viewTf]?.active) {
    return windows[viewTf];
  }
  if (viewTf !== "5m" && windows?.["5m"]?.active) {
    return windows["5m"];
  }
  return null;
}

/** Merge window view into snapshot shape expected by overview/live components. */
export function viewSnapshot(snap: Record<string, unknown>, viewTf: string): Record<string, unknown> {
  const w = pickWindow(snap, viewTf);
  if (!w?.active) {
    return {
      ...snap,
      current_market: undefined,
      signal: undefined,
      strategy: undefined,
      next_market: undefined,
      last_decision: undefined,
      view_timeframe: viewTf,
      view_active: false,
    };
  }
  return {
    ...snap,
    current_market: w.market,
    signal: w.signal,
    strategy: w.strategy,
    next_market: w.next_market,
    last_decision: w.last_decision,
    view_timeframe: viewTf,
    view_active: true,
  };
}

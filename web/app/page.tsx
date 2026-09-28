"use client";

import { useCallback, useEffect, useState } from "react";

type Status = { heartbeat?: Record<string, unknown> };
type Pnl = { total_pnl?: string; win_rate?: number };
type Report = { slug: string; result: string; pnl: string; fees: string; closed_at: string };

export default function Home() {
  const [status, setStatus] = useState<Status>({});
  const [pnl, setPnl] = useState<Pnl>({});
  const [reports, setReports] = useState<Report[]>([]);

  const refresh = useCallback(async () => {
    const [s, p, r] = await Promise.all([
      fetch("/api/status").then((x) => x.json()),
      fetch("/api/pnl").then((x) => x.json()),
      fetch("/api/reports?limit=20").then((x) => x.json()),
    ]);
    setStatus(s);
    setPnl(p);
    setReports(r.reports ?? []);
  }, []);

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 2000);
    return () => clearInterval(t);
  }, [refresh]);

  async function activateKill() {
    await fetch("/api/kill", { method: "POST" });
    refresh();
  }

  return (
    <main>
      <section style={{ display: "grid", gap: 16, gridTemplateColumns: "1fr 1fr" }}>
        <Card title="System status">
          <pre style={{ fontSize: 12, overflow: "auto" }}>{JSON.stringify(status, null, 2)}</pre>
        </Card>
        <Card title="PnL">
          <p>Total PnL: {pnl.total_pnl ?? "—"}</p>
          <p>Win rate: {pnl.win_rate != null ? `${(pnl.win_rate * 100).toFixed(1)}%` : "—"}</p>
          <button type="button" onClick={activateKill} style={{ marginTop: 12, padding: "8px 12px" }}>
            Activate kill switch
          </button>
        </Card>
      </section>
      <section style={{ marginTop: 24 }}>
        <Card title="Market reports">
          <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 14 }}>
            <thead>
              <tr>
                <th align="left">Slug</th>
                <th align="left">Result</th>
                <th align="right">PnL</th>
                <th align="right">Fees</th>
              </tr>
            </thead>
            <tbody>
              {reports.map((r) => (
                <tr key={r.slug}>
                  <td>{r.slug}</td>
                  <td>{r.result}</td>
                  <td align="right">{r.pnl}</td>
                  <td align="right">{r.fees}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      </section>
    </main>
  );
}

function Card({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div style={{ border: "1px solid #30363d", borderRadius: 8, padding: 16 }}>
      <h2 style={{ marginTop: 0, fontSize: 16 }}>{title}</h2>
      {children}
    </div>
  );
}

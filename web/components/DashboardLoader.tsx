"use client";

import dynamic from "next/dynamic";

const Dashboard = dynamic(() => import("./Dashboard"), {
  ssr: false,
  loading: () => (
    <main style={{ padding: 24 }}>
      <p>Loading dashboard…</p>
    </main>
  ),
});

export default function DashboardLoader() {
  return <Dashboard />;
}

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body style={{ fontFamily: "system-ui", margin: 24, background: "#0b0f14", color: "#e6edf3" }}>
        <header style={{ marginBottom: 24 }}>
          <h1>Polymarket Crypto SDK</h1>
          <p style={{ opacity: 0.7 }}>Read-only admin dashboard</p>
        </header>
        {children}
      </body>
    </html>
  );
}

import type { NextConfig } from "next";

const dashboardPort = process.env.DASHBOARD_API_PORT ?? "8081";

const nextConfig: NextConfig = {
  async rewrites() {
    return [
      {
        source: "/api/:path*",
        destination: `http://127.0.0.1:${dashboardPort}/api/:path*`,
      },
    ];
  },
};

export default nextConfig;

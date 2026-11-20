import type { NextConfig } from "next";

// Proxy /api and /download to the Rust backend so the browser talks
// same-origin only. Override with BACKEND_URL when deploying.
const backend = process.env.BACKEND_URL ?? "http://localhost:8080";

const nextConfig: NextConfig = {
  eslint: { ignoreDuringBuilds: true },
  async rewrites() {
    return [
      { source: "/api/:path*", destination: `${backend}/api/:path*` },
      { source: "/download", destination: `${backend}/download` },
    ];
  },
};

export default nextConfig;

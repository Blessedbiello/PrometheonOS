import type { NextConfig } from "next";

const config: NextConfig = {
  reactStrictMode: true,
  // `nats` is a Node TCP client used by the live telemetry bridge; keep it external (unbundled).
  serverExternalPackages: ["nats"],
};

export default config;

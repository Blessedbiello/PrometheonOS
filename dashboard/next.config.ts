import type { NextConfig } from "next";

const config: NextConfig = {
  reactStrictMode: true,
  // `nats` is a Node TCP client used by the live telemetry bridge; keep it external (unbundled).
  serverExternalPackages: ["nats"],
  // Bundle the committed proof-replay artifacts into the telemetry route's serverless function so the
  // proof-replay source works on Vercel (where files outside the app root aren't traced). The data is
  // copied into proof-data/ from the repo-root logs/ at build time — see scripts/copy-proof-data.mjs.
  outputFileTracingIncludes: {
    "/api/telemetry": ["./proof-data/**/*"],
  },
};

export default config;

/**
 * PrometheonOS AI agent entrypoint.
 *
 * Selects the configured `LlmProvider`, connects to NATS, and serves `decision.request.*` with
 * structured, schema-validated decisions carrying visible reasoning.
 */
import "dotenv/config";
import { providerFromEnv } from "./providers/index.js";
import { runAgent } from "./nats.js";

export const VERSION = "0.1.0";

async function main(): Promise<void> {
  const provider = providerFromEnv();
  const natsUrl = process.env.NATS_URL ?? "nats://localhost:4222";
  // eslint-disable-next-line no-console
  console.log(`PrometheonOS AI agent v${VERSION} — provider=${provider.name}, nats=${natsUrl}`);
  const nc = await runAgent(provider, natsUrl);
  await nc.closed();
}

main().catch((err) => {
  // eslint-disable-next-line no-console
  console.error("agent failed:", err);
  process.exit(1);
});

/**
 * PrometheonOS AI agent entrypoint.
 *
 * Subscribes to `decision.request.*` and telemetry on NATS, runs the configured
 * `LlmProvider` (anthropic | openai | ollama), and replies with structured, schema-validated
 * decisions carrying visible reasoning. Skeleton only — implemented test-first in Phase 5.
 */

export const VERSION = "0.1.0";

function main(): void {
  // eslint-disable-next-line no-console
  console.log(`PrometheonOS AI agent v${VERSION} — scaffold (Phase 5 not yet implemented)`);
}

main();

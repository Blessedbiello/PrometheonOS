/**
 * Capture a REAL AI reasoning trace from the configured provider — the funding-free way to prove the
 * model genuinely drives the operational decision (not the mock). Bypasses NATS: it calls the same
 * `decide()` the agent serves, over the exact `tip` + `retry` contexts the Rust core sends, and writes
 * a committable markdown artifact to `logs/ai-decision-trace.md`.
 *
 *   # any OpenAI-compatible host (Groq shown), or LLM_PROVIDER=anthropic|ollama
 *   LLM_PROVIDER=openai OPENAI_BASE_URL=https://api.groq.com/openai/v1 \
 *     OPENAI_API_KEY=… OPENAI_MODEL=llama-3.3-70b-versatile \
 *     pnpm --filter @prometheon/ai-agent trace
 */
import { config } from "dotenv";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { mkdirSync, writeFileSync } from "node:fs";
import { providerFromEnv } from "./providers/index.js";
import { decide } from "./agent.js";
import type { Decision } from "./schema.js";
import type { DecisionRequest } from "./providers/types.js";

const here = dirname(fileURLToPath(import.meta.url)); // ai-agent/src
config({ path: resolve(here, "../../.env") }); // load the repo-root .env regardless of cwd

// The exact contexts the Rust core publishes (camelCase) — see submission::tip_context /
// saga::retry_context — so the trace mirrors a real run.
const REQUESTS: DecisionRequest[] = [
  {
    decisionType: "tip",
    context: {
      congestionScore: 0.62,
      slotStabilityScore: 0.88,
      bundleLandingProbability: 0.7,
      tipFloorP50Lamports: 14_200,
      avgConfirmedLatencyMs: 920,
      recentFailures: 1,
      lastTipLamports: 12_000,
    },
  },
  {
    decisionType: "retry",
    context: {
      failureClass: "ExpiredBlockhash",
      attempt: 1,
      lastTipLamports: 14_500,
      congestionScore: 0.74,
      tipFloorP50Lamports: 16_800,
      slotStabilityScore: 0.71,
    },
  },
];

function render(provider: string, decisions: Decision[]): string {
  let out = `# PrometheonOS — Real AI Decision Trace\n\n`;
  out += `Captured from the live \`${provider}\` provider (no mock). These are real model decisions over `;
  out += `the exact tip/retry contexts the Rust core sends; each must satisfy the causal contract `;
  out += `(\`after.tip\` for tip, \`after.{tip,refresh_blockhash}\` for retry) or the agent rejects it.\n\n`;
  for (const d of decisions) {
    out += `## ${d.decision_type} decision\n\n`;
    out += `**Action:** ${d.action}\n\n`;
    out += `**Confidence:** ${d.confidence.toFixed(2)} · **Provider:** ${d.provider} · **Latency:** ${d.latency_ms} ms\n\n`;
    out += `**Reasoning:** ${d.reasoning}\n\n`;
    out += `\`\`\`json\n${JSON.stringify({ before: d.before, after: d.after, inputs: d.inputs_considered }, null, 2)}\n\`\`\`\n\n`;
  }
  return out;
}

async function main(): Promise<void> {
  const provider = providerFromEnv();
  const decisions: Decision[] = [];
  for (const req of REQUESTS) {
    const d = await decide(provider, req);
    decisions.push(d);
    console.log(`✓ ${d.decision_type}: ${d.action} (conf ${d.confidence}, ${d.latency_ms}ms)`);
  }
  const md = render(provider.name, decisions);
  const outDir = resolve(here, "../../logs");
  mkdirSync(outDir, { recursive: true });
  const outPath = resolve(outDir, "ai-decision-trace.md");
  writeFileSync(outPath, md);
  console.log(`\nwrote ${outPath}`);
}

main().catch((err) => {
  console.error("trace failed:", err instanceof Error ? err.message : err);
  process.exit(1);
});

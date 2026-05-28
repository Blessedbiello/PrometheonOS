/**
 * The agent orchestrator: run a provider for one decision, validate its output, and assemble the
 * full `Decision` (adding provider name, latency, timestamp, and the inputs considered).
 */
import { decisionSchema, llmDecisionSchema, type Decision } from "./schema.js";
import type { DecisionRequest, LlmProvider } from "./providers/types.js";

/**
 * Produce a validated, fully-assembled `Decision`. Throws if the provider returns an output that
 * fails `llmDecisionSchema` (empty fields, out-of-range confidence) or if assembly is invalid.
 */
export async function decide(provider: LlmProvider, req: DecisionRequest): Promise<Decision> {
  const start = Date.now();
  const raw = await provider.decide(req);
  const llm = llmDecisionSchema.parse(raw); // reject malformed model output early
  const latency_ms = Date.now() - start;

  return decisionSchema.parse({
    decision_type: req.decisionType,
    action: llm.action,
    reasoning: llm.reasoning,
    confidence: llm.confidence,
    inputs_considered: req.context,
    before: llm.before ?? null,
    after: llm.after ?? null,
    provider: provider.name,
    latency_ms,
    ts: new Date().toISOString(),
  } satisfies Decision);
}

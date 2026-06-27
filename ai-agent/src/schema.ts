/**
 * Runtime schemas. `decisionSchema` mirrors the Rust `Decision` contract
 * (`prometheon-telemetry::decision::Decision`) so the wire format is identical across languages.
 */
import { z } from "zod";

export const decisionTypeSchema = z.enum(["tip", "timing", "retry"]);

/** What a provider/LLM must return. */
export const llmDecisionSchema = z.object({
  action: z.string().min(1),
  reasoning: z.string().min(1),
  confidence: z.number().min(0).max(1),
  before: z.unknown().optional(),
  after: z.unknown().optional(),
});

/**
 * The CAUSAL fields the deterministic Rust core actually reads from a decision's `after`. They are
 * enforced ([`requireCausalAfter`]) so the model's choice genuinely drives the on-chain action — never
 * a silent fallback to deterministic policy while still emitting decorative "reasoning".
 */
export const tipAfterSchema = z.object({ tip: z.number().int().positive() }).passthrough();
export const retryAfterSchema = z
  .object({ tip: z.number().int().positive(), refresh_blockhash: z.boolean() })
  .passthrough();

/** Throw unless the decision carries the exact `after` keys the core consumes for its type. */
export function requireCausalAfter(decisionType: DecisionType, after: unknown): void {
  if (decisionType === "tip") {
    if (!tipAfterSchema.safeParse(after).success) {
      throw new Error('tip decision must include after.tip as an integer (lamports)');
    }
  } else if (decisionType === "retry") {
    if (!retryAfterSchema.safeParse(after).success) {
      throw new Error(
        'retry decision must include after.tip (integer lamports) and after.refresh_blockhash (boolean)',
      );
    }
  }
  // timing is advisory — the core consumes no causal field from it (yet).
}

/** The fully-assembled decision published to NATS / persisted (snake_case mirrors the Rust struct). */
export const decisionSchema = z.object({
  decision_type: decisionTypeSchema,
  action: z.string(),
  reasoning: z.string(),
  confidence: z.number().min(0).max(1),
  inputs_considered: z.unknown(),
  before: z.unknown().nullable(),
  after: z.unknown().nullable(),
  provider: z.string(),
  latency_ms: z.number().int().nonnegative(),
  ts: z.string(),
});

export type DecisionType = z.infer<typeof decisionTypeSchema>;
export type LlmDecisionParsed = z.infer<typeof llmDecisionSchema>;
export type Decision = z.infer<typeof decisionSchema>;

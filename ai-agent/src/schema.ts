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

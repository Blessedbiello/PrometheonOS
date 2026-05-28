/** Provider abstraction: the swappable LLM backend the agent reasons through. */

/** The operational decision the agent is being asked to make. */
export type DecisionType = "tip" | "timing" | "retry";

/** A request for a decision: the type plus a structured context snapshot. */
export interface DecisionRequest {
  decisionType: DecisionType;
  /** Inputs the model should reason over (health snapshot, tip floor, failure classification, …). */
  context: Record<string, unknown>;
}

/** The model's structured reasoning output (provider-independent). */
export interface LlmDecision {
  /** Short, human-readable action, e.g. `"tip 12000->18000 lamports"`. */
  action: string;
  /** The model's reasoning. */
  reasoning: string;
  /** Confidence in `[0,1]`. */
  confidence: number;
  /** Optional before/after state for comparison. */
  before?: unknown;
  after?: unknown;
}

/** A pluggable LLM provider. All providers return the same `LlmDecision` shape. */
export interface LlmProvider {
  readonly name: string;
  decide(req: DecisionRequest): Promise<LlmDecision>;
}

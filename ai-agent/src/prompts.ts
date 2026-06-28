/**
 * Shared prompt construction. All providers build the prompt the same way, so the reasoning is a
 * function of our prompt + the model — not a single vendor's idiosyncrasies. The model is required
 * to return ONLY a JSON object matching the `LlmDecision` shape.
 */
import type { DecisionRequest, DecisionType } from "./providers/types.js";

const ROLE =
  "You are the autonomous execution strategist for a Solana transaction infrastructure engine. " +
  "You make one operational decision at a time from live network telemetry and explain it.";

const OUTPUT_CONTRACT =
  'Return ONLY a single JSON object, no prose, with these keys: ' +
  '"action" (string, a short imperative summary), ' +
  '"reasoning" (string, why — cite the telemetry), ' +
  '"confidence" (number 0..1), ' +
  'plus the required "after" object below (and optionally "before"). ' +
  "Do not wrap the JSON in markdown.";

function guidance(decisionType: DecisionType): string {
  switch (decisionType) {
    case "tip":
      return (
        "Decide the Jito tip (lamports) for the next bundle. Balance cost against landing " +
        "probability using the tip-floor percentiles and the congestion score. The landed-tip " +
        "distribution is heavily skewed: tipFloorP50Lamports sits at the ~1000-lamport Jito noise " +
        "floor, so tipping near P50 rarely wins inclusion. To land reliably, anchor on the P75–P95 " +
        "band (tipFloorP75Lamports..tipFloorP95Lamports), leaning toward P95 as congestion rises. " +
        "Never hardcode; derive the tip from the live floor."
      );
    case "timing":
      return (
        "Decide whether to submit now or hold. Consider the leader schedule (is the next leader a " +
        "Jito leader?), slot stability, and congestion. Holding trades latency for a better window."
      );
    case "retry":
      return (
        "Decide whether and how to retry a failed submission. If the blockhash expired, refresh it " +
        "and recalculate the tip before resubmitting. Justify retryability from the failure class."
      );
  }
}

/**
 * The EXACT `after` shape the deterministic core reads for this decision type. The engine acts on
 * these keys, so they are mandatory (the agent rejects a reply that omits them) — this is what makes
 * the model's number causally drive the action rather than being decorative.
 */
function afterContract(decisionType: DecisionType): string {
  switch (decisionType) {
    case "tip":
      return 'REQUIRED: "after": { "tip": <integer lamports the engine will use> }.';
    case "retry":
      return 'REQUIRED: "after": { "refresh_blockhash": <true|false>, "tip": <integer lamports> } — the engine reads these exact keys.';
    case "timing":
      return 'Optionally include "after": { "hold": <true|false> } describing the timing choice.';
  }
}

/** Build the `{ system, user }` prompt for a decision request. */
export function buildPrompt(req: DecisionRequest): { system: string; user: string } {
  const system = `${ROLE}\n\n${guidance(req.decisionType)}\n\n${OUTPUT_CONTRACT}\n\n${afterContract(req.decisionType)}`;
  const user =
    `Decision type: ${req.decisionType}\n` +
    `Telemetry (JSON):\n${JSON.stringify(req.context, null, 2)}\n\n` +
    "Respond with the JSON decision object now.";
  return { system, user };
}

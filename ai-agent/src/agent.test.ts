import { describe, expect, it } from "vitest";
import { decide } from "./agent.js";
import { decisionSchema } from "./schema.js";
import { MockProvider } from "./providers/mock.js";
import type { DecisionRequest, LlmDecision, LlmProvider } from "./providers/types.js";

describe("decide()", () => {
  it("composes a schema-valid Decision carrying provider, latency, ts, and inputs", async () => {
    const provider = new MockProvider();
    const req: DecisionRequest = {
      decisionType: "tip",
      context: { tipFloorP50Lamports: 14200, congestionScore: 0.74 },
    };
    const d = await decide(provider, req);

    expect(decisionSchema.safeParse(d).success).toBe(true);
    expect(d.decision_type).toBe("tip");
    expect(d.provider).toBe("mock");
    expect(d.inputs_considered).toEqual(req.context);
    expect(d.latency_ms).toBeGreaterThanOrEqual(0);
    expect(() => new Date(d.ts)).not.toThrow();
    // null (not undefined) when the model omits before/after — matches the Rust Option<Value>.
    expect(d.before === null || typeof d.before === "object").toBe(true);
  });

  it("rejects a provider that returns an invalid decision", async () => {
    const bad: LlmProvider = {
      name: "bad",
      decide: async (_req: DecisionRequest): Promise<LlmDecision> => ({
        action: "",
        reasoning: "",
        confidence: 5,
      }),
    };
    await expect(decide(bad, { decisionType: "retry", context: {} })).rejects.toThrow();
  });
});

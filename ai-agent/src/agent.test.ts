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

  it("rejects a tip decision missing the causal after.tip (no decorative fallback)", async () => {
    const noTip: LlmProvider = {
      name: "x",
      decide: async (): Promise<LlmDecision> => ({ action: "tip", reasoning: "r", confidence: 0.5 }),
    };
    await expect(decide(noTip, { decisionType: "tip", context: {} })).rejects.toThrow(/after\.tip/);
  });

  it("rejects a retry decision missing after.refresh_blockhash", async () => {
    const noRefresh: LlmProvider = {
      name: "x",
      decide: async (): Promise<LlmDecision> => ({
        action: "retry",
        reasoning: "r",
        confidence: 0.5,
        after: { tip: 25_000 },
      }),
    };
    await expect(decide(noRefresh, { decisionType: "retry", context: {} })).rejects.toThrow(
      /refresh_blockhash/,
    );
  });

  it("mock retry now carries the causal after fields the core consumes", async () => {
    const d = await decide(new MockProvider(), {
      decisionType: "retry",
      context: { tipFloorP50Lamports: 14_200, congestionScore: 0.5, lastTipLamports: 12_000 },
    });
    expect((d.after as { refresh_blockhash: boolean }).refresh_blockhash).toBe(true);
    expect(typeof (d.after as { tip: number }).tip).toBe("number");
  });
});

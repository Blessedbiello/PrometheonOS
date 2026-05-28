import { describe, expect, it } from "vitest";
import { llmDecisionSchema } from "../schema.js";
import { MockProvider } from "./mock.js";

describe("MockProvider", () => {
  it("is named and deterministic", async () => {
    const p = new MockProvider();
    expect(p.name).toBe("mock");
    const a = await p.decide({ decisionType: "tip", context: { tipFloorP50Lamports: 14200, congestionScore: 0.5 } });
    const b = await p.decide({ decisionType: "tip", context: { tipFloorP50Lamports: 14200, congestionScore: 0.5 } });
    expect(a).toEqual(b);
  });

  it("returns schema-valid decisions for every decision type", async () => {
    const p = new MockProvider();
    for (const decisionType of ["tip", "timing", "retry"] as const) {
      const d = await p.decide({ decisionType, context: { congestionScore: 0.3 } });
      expect(llmDecisionSchema.safeParse(d).success).toBe(true);
    }
  });

  it("reacts to congestion in the tip action (so flow tests are meaningful)", async () => {
    const p = new MockProvider();
    const calm = await p.decide({ decisionType: "tip", context: { tipFloorP50Lamports: 10000, congestionScore: 0.0 } });
    const hot = await p.decide({ decisionType: "tip", context: { tipFloorP50Lamports: 10000, congestionScore: 1.0 } });
    // Higher congestion ⇒ the mock proposes a higher tip (confidence stays valid).
    const tipOf = (s: string) => Number(s.match(/(\d+)\s*lamports/)?.[1] ?? "0");
    expect(tipOf(hot.action)).toBeGreaterThan(tipOf(calm.action));
  });
});

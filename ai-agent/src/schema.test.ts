import { describe, expect, it } from "vitest";
import { decisionSchema, llmDecisionSchema } from "./schema.js";

describe("llmDecisionSchema", () => {
  it("accepts a well-formed model decision", () => {
    const ok = llmDecisionSchema.safeParse({
      action: "tip 12000->18000 lamports",
      reasoning: "congestion rising; recent 12k bundles missed",
      confidence: 0.81,
    });
    expect(ok.success).toBe(true);
  });

  it("rejects confidence outside [0,1]", () => {
    expect(llmDecisionSchema.safeParse({ action: "a", reasoning: "r", confidence: 1.5 }).success).toBe(false);
    expect(llmDecisionSchema.safeParse({ action: "a", reasoning: "r", confidence: -0.1 }).success).toBe(false);
  });

  it("rejects empty action or reasoning", () => {
    expect(llmDecisionSchema.safeParse({ action: "", reasoning: "r", confidence: 0.5 }).success).toBe(false);
    expect(llmDecisionSchema.safeParse({ action: "a", reasoning: "", confidence: 0.5 }).success).toBe(false);
  });
});

describe("decisionSchema (mirrors the Rust Decision contract)", () => {
  it("accepts a fully-assembled decision", () => {
    const ok = decisionSchema.safeParse({
      decision_type: "tip",
      action: "tip 12000->18000",
      reasoning: "because",
      confidence: 0.8,
      inputs_considered: { congestion: 0.74 },
      before: { tip: 12000 },
      after: { tip: 18000 },
      provider: "mock",
      latency_ms: 12,
      ts: new Date().toISOString(),
    });
    expect(ok.success).toBe(true);
  });

  it("requires a known decision_type and snake_case fields", () => {
    const bad = decisionSchema.safeParse({
      decision_type: "unknown",
      action: "x",
      reasoning: "y",
      confidence: 0.5,
      inputs_considered: {},
      before: null,
      after: null,
      provider: "mock",
      latency_ms: 0,
      ts: new Date().toISOString(),
    });
    expect(bad.success).toBe(false);
  });

  it("rejects negative latency", () => {
    const bad = decisionSchema.safeParse({
      decision_type: "retry",
      action: "x",
      reasoning: "y",
      confidence: 0.5,
      inputs_considered: {},
      before: null,
      after: null,
      provider: "mock",
      latency_ms: -5,
      ts: new Date().toISOString(),
    });
    expect(bad.success).toBe(false);
  });
});

import { describe, expect, it } from "vitest";
import { buildPrompt } from "./prompts.js";

describe("buildPrompt", () => {
  it("instructs JSON-only output and includes the context", () => {
    const { system, user } = buildPrompt({
      decisionType: "tip",
      context: { congestionScore: 0.74, tipFloorP50Lamports: 14200 },
    });
    expect(system.toLowerCase()).toContain("json");
    // The required output keys are spelled out for the model.
    expect(system).toContain("action");
    expect(system).toContain("reasoning");
    expect(system).toContain("confidence");
    // The context is serialized into the user prompt.
    expect(user).toContain("14200");
    expect(user).toContain("congestionScore");
  });

  it("gives decision-type-specific guidance", () => {
    const tip = buildPrompt({ decisionType: "tip", context: {} });
    expect(tip.system.toLowerCase()).toContain("tip");
    const timing = buildPrompt({ decisionType: "timing", context: {} });
    expect(timing.system.toLowerCase()).toMatch(/submit|hold|timing|leader/);
    const retry = buildPrompt({ decisionType: "retry", context: {} });
    expect(retry.system.toLowerCase()).toMatch(/retry|blockhash|resubmit/);
  });
});

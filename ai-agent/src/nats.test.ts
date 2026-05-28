import { describe, expect, it } from "vitest";
import { handleDecisionRequest } from "./nats.js";
import { decisionSchema } from "./schema.js";
import { MockProvider } from "./providers/mock.js";

describe("handleDecisionRequest", () => {
  it("parses a request, decides, and returns a serialized Decision", async () => {
    const raw = JSON.stringify({
      decisionType: "tip",
      context: { tipFloorP50Lamports: 14200, congestionScore: 0.74 },
    });
    const out = await handleDecisionRequest(new MockProvider(), raw);
    const parsed = decisionSchema.parse(JSON.parse(out));
    expect(parsed.decision_type).toBe("tip");
    expect(parsed.provider).toBe("mock");
    expect(parsed.inputs_considered).toEqual({ tipFloorP50Lamports: 14200, congestionScore: 0.74 });
  });

  it("rejects a malformed request payload", async () => {
    await expect(handleDecisionRequest(new MockProvider(), "not json")).rejects.toThrow();
    await expect(handleDecisionRequest(new MockProvider(), '{"context":{}}')).rejects.toThrow();
  });
});

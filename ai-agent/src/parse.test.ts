import { describe, expect, it } from "vitest";
import { parseLlmDecision } from "./parse.js";

describe("parseLlmDecision", () => {
  it("parses a raw JSON object", () => {
    const d = parseLlmDecision('{"action":"tip 18000 lamports","reasoning":"congestion","confidence":0.8}');
    expect(d.action).toBe("tip 18000 lamports");
    expect(d.confidence).toBe(0.8);
  });

  it("strips markdown code fences", () => {
    const text = '```json\n{"action":"hold","reasoning":"unstable","confidence":0.6}\n```';
    expect(parseLlmDecision(text).action).toBe("hold");
  });

  it("extracts the JSON object from surrounding prose", () => {
    const text = 'Here is my decision:\n{"action":"submit now","reasoning":"calm","confidence":0.7}\nThanks!';
    expect(parseLlmDecision(text).action).toBe("submit now");
  });

  it("throws on invalid (out-of-range confidence)", () => {
    expect(() => parseLlmDecision('{"action":"x","reasoning":"y","confidence":2}')).toThrow();
  });

  it("throws when no JSON object is present", () => {
    expect(() => parseLlmDecision("the model refused")).toThrow();
  });
});

import { describe, expect, it } from "vitest";
import {
  confidenceBars,
  formatLamports,
  formatMs,
  formatPct,
  formatSlot,
  lamportsToSol,
  stageColor,
  truncId,
} from "../lib/format.js";

describe("formatters", () => {
  it("lamports + SOL", () => {
    expect(formatLamports(18000)).toBe("18,000 lamports");
    expect(lamportsToSol(1_000_000_000, 2)).toBe("1.00 SOL");
    expect(lamportsToSol(50_000, 6)).toBe("0.000050 SOL");
  });

  it("formatMs covers sub-second, seconds, and missing values", () => {
    expect(formatMs(612)).toBe("612ms");
    expect(formatMs(1_200)).toBe("1.20s");
    expect(formatMs(13_300)).toBe("13.30s");
    expect(formatMs(null)).toBe("—");
    expect(formatMs(undefined)).toBe("—");
  });

  it("formatPct rounds to a whole percent", () => {
    expect(formatPct(0.74)).toBe("74%");
    expect(formatPct(1)).toBe("100%");
    expect(formatPct(0)).toBe("0%");
    expect(formatPct(null)).toBe("—");
  });

  it("truncId keeps short ids as-is", () => {
    expect(truncId("a3f9b6e21c0d2")).toBe("a3f9b6…c0d2");
    expect(truncId("short")).toBe("short");
  });

  it("formatSlot uses thousands separators", () => {
    expect(formatSlot(298_431_204)).toBe("298,431,204");
  });

  it("stageColor maps lifecycle stages to status colors", () => {
    expect(stageColor("finalized")).toBe("green");
    expect(stageColor("confirmed")).toBe("blue");
    expect(stageColor("processed")).toBe("yellow");
    expect(stageColor("failed")).toBe("red");
    expect(stageColor("expired")).toBe("red");
    expect(stageColor("submitted")).toBe("gray");
  });

  it("confidenceBars renders 5-segment bars proportionally", () => {
    expect(confidenceBars(0)).toBe("░░░░░");
    expect(confidenceBars(0.5)).toBe("███░░");
    expect(confidenceBars(0.81)).toBe("████░");
    expect(confidenceBars(1)).toBe("█████");
  });
});

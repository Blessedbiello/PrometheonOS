import { describe, expect, it } from "vitest";
import { MockState } from "../lib/mock.js";

describe("MockState", () => {
  it("advances the slot every tick and is deterministic for a given seed", () => {
    const a = new MockState(42);
    const b = new MockState(42);
    const snapA = a.tick();
    const snapB = b.tick();
    expect(snapA.current_slot).toBe(snapB.current_slot);
    expect(snapA.health.slot_stability_score).toBe(snapB.health.slot_stability_score);
  });

  it("produces a structurally complete snapshot", () => {
    const s = new MockState(7);
    for (let i = 0; i < 100; i++) s.tick();
    const snap = s.tick();
    expect(snap.current_slot).toBeGreaterThan(298_430_000);
    expect(snap.source).toBe("mock"); // the UI must render this as "simulated", never "live"
    expect(snap.network).toBe("mock");
    expect(snap.health.congestion_score).toBeGreaterThanOrEqual(0);
    expect(snap.health.congestion_score).toBeLessThanOrEqual(1);
    expect(snap.health.slot_stability_score).toBeGreaterThanOrEqual(0);
    expect(snap.health.slot_stability_score).toBeLessThanOrEqual(1);
    expect(snap.slots.length).toBeGreaterThan(0);
    // We expect at least some bundles and decisions to have been generated.
    expect(snap.bundles.length).toBeGreaterThan(0);
    expect(snap.decisions.length).toBeGreaterThan(0);
  });

  it("bundles progress through the lifecycle stages", () => {
    const s = new MockState(11);
    let sawProcessed = false;
    let sawConfirmed = false;
    let sawFinalized = false;
    for (let i = 0; i < 500; i++) {
      const snap = s.tick();
      for (const b of snap.bundles) {
        if (b.stage === "processed") sawProcessed = true;
        if (b.stage === "confirmed") sawConfirmed = true;
        if (b.stage === "finalized") sawFinalized = true;
      }
    }
    expect(sawProcessed).toBe(true);
    expect(sawConfirmed).toBe(true);
    expect(sawFinalized).toBe(true);
  });

  it("emits AI decisions with reasoning and confidence in [0,1]", () => {
    const s = new MockState(3);
    let totalDecisions = 0;
    for (let i = 0; i < 200; i++) {
      const snap = s.tick();
      for (const d of snap.decisions) {
        totalDecisions++;
        expect(d.reasoning.length).toBeGreaterThan(0);
        expect(d.confidence).toBeGreaterThanOrEqual(0);
        expect(d.confidence).toBeLessThanOrEqual(1);
        expect(["tip", "timing", "retry"]).toContain(d.decision_type);
      }
    }
    expect(totalDecisions).toBeGreaterThan(0);
  });
});

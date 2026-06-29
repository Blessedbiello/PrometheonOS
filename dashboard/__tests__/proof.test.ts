import { describe, expect, it } from "vitest";
import { ProofReplay } from "../lib/proof";

/** Build a snapshot `elapsedMs` into the replay using a controllable clock. */
function snapshotAt(elapsedMs: number) {
  let t = 1_000_000;
  const r = new ProofReplay(() => t);
  r.snapshot(); // first call pins startedAt = t
  t += elapsedMs;
  return r.snapshot();
}

describe("proof-replay source (committed mainnet run)", () => {
  it("at the final frame, renders the whole committed run with two AI recovery chains", () => {
    const snap = snapshotAt(36_000); // past DURATION → the held final frame

    expect(snap.source).toBe("proof");
    expect(snap.network).toBe("mainnet");
    expect(snap.bundles.length).toBe(14);

    const landed = snap.bundles.filter((b) => b.stage === "finalized" || b.stage === "confirmed");
    expect(landed.length).toBe(12);

    // Exactly two classified failures, one of each injected kind.
    const fails = snap.bundles.filter((b) => b.failure_class != null);
    expect(fails.length).toBe(2);
    expect(new Set(fails.map((b) => b.failure_class))).toEqual(
      new Set(["fee_too_low", "expired_blockhash"]),
    );

    // Each recovery chain: a failed attempt 1 linked (by base_id) to a landed attempt 2.
    for (const base of ["b11", "b12"]) {
      const group = snap.bundles.filter((b) => b.base_id === base);
      expect(group.length).toBe(2);
      expect(group.some((b) => b.failure_class != null && b.attempt === 1)).toBe(true);
      expect(
        group.some(
          (b) => (b.stage === "finalized" || b.stage === "confirmed") && (b.attempt ?? 0) >= 2,
        ),
      ).toBe(true);
    }

    // All 15 decisions are real AI (openai), never the deterministic fallback.
    expect(snap.decisions.length).toBe(15);
    expect(snap.decisions.every((d) => d.provider === "openai")).toBe(true);
    expect(snap.decisions.filter((d) => d.decision_type === "retry").length).toBe(2);

    // The two retries are divergent remedies (the causal-contract proof).
    const retries = snap.decisions.filter((d) => d.decision_type === "retry");
    const refreshFlags = retries.map((d) => (d.after as Record<string, unknown>)?.refresh_blockhash);
    expect(new Set(refreshFlags)).toEqual(new Set([true, false]));
  });

  it("at the start of the replay, the story has not yet unfolded", () => {
    const snap = snapshotAt(0);
    expect(snap.bundles.filter((b) => b.failure_class != null).length).toBe(0);
    expect(snap.bundles.length).toBeLessThanOrEqual(3);
  });
});

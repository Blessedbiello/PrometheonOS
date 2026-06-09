import { describe, expect, it } from "vitest";
import { LiveTelemetry } from "../lib/live.js";

/** A controllable clock so freshness assertions are deterministic. */
function clock(start = 1_000): { now: () => number; set: (t: number) => void } {
  let t = start;
  return { now: () => t, set: (v: number) => (t = v) };
}

describe("LiveTelemetry reducer", () => {
  it("folds slot events into a bounded, current-slot-tracking window", () => {
    const live = new LiveTelemetry();
    for (let i = 0; i < 25; i++) {
      live.apply({
        kind: "slot",
        slot: 425_000_000 + i,
        parent: 425_000_000 + i - 1,
        status: "confirmed",
        ts: "2026-06-09T00:00:00Z",
      });
    }
    const snap = live.snapshot();
    expect(snap.slots.length).toBe(20); // window bound
    expect(snap.current_slot).toBe(425_000_024);
    expect(snap.slots.at(-1)?.slot).toBe(425_000_024);
  });

  it("maps a health snapshot and defaults the dashboard-only delta field", () => {
    const live = new LiveTelemetry();
    live.apply({
      kind: "health",
      ts: "2026-06-09T00:00:00Z",
      congestion_score: 0.42,
      slot_stability_score: 0.9,
      bundle_landing_probability: 0.8,
      retry_success_rate: 0.5,
      tip_efficiency_ratio: 0,
      cost_per_successful_landing: null,
      avg_confirmed_latency_ms: 850,
      confirm_latency_variance_ms: null,
      tip_floor_lamports: 2856,
    });
    const h = live.snapshot().health;
    expect(h.congestion_score).toBe(0.42);
    expect(h.tip_floor_lamports).toBe(2856);
    expect(h.processed_to_confirmed_delta_ms).toBeNull();
  });

  it("captures AI decisions and surfaces the provider", () => {
    const live = new LiveTelemetry();
    live.apply({
      kind: "decision",
      decision_type: "tip",
      action: "tip 14500 lamports",
      reasoning: "congestion rising",
      confidence: 0.7,
      inputs_considered: { congestionScore: 0.9 },
      before: { tip: 12500 },
      after: { tip: 14500 },
      provider: "anthropic",
      latency_ms: 740,
      ts: "2026-06-09T00:00:00Z",
    });
    const snap = live.snapshot();
    expect(snap.ai_provider).toBe("anthropic");
    expect(snap.decisions[0]?.action).toBe("tip 14500 lamports");
    expect(snap.decisions[0]?.after).toEqual({ tip: 14500 });
  });

  it("threads a bundle through its lifecycle and computes latencies", () => {
    const live = new LiveTelemetry();
    live.apply({
      kind: "bundle",
      bundle_id: "abc",
      tip_lamports: 14500,
      tip_account: "Tip1",
      region: "ny",
      signatures: ["sig1"],
      phase: "submitted",
      ts: "2026-06-09T00:00:00.000Z",
    });
    live.apply({
      kind: "lifecycle",
      id: "abc",
      event: { stage: "processed", slot: 425_000_100, ts: "2026-06-09T00:00:00.500Z", delta_ms_from_prev: 500 },
    });
    live.apply({
      kind: "lifecycle",
      id: "abc",
      event: { stage: "confirmed", slot: 425_000_100, ts: "2026-06-09T00:00:01.700Z", delta_ms_from_prev: 1200 },
    });
    const b = live.snapshot().bundles[0]!;
    expect(b.bundle_id).toBe("abc");
    expect(b.stage).toBe("confirmed");
    expect(b.slot).toBe(425_000_100);
    expect(b.latencies.processed_ms).toBe(500);
    expect(b.latencies.confirmed_ms).toBe(1700);
  });

  it("marks a failed bundle with its classification", () => {
    const live = new LiveTelemetry();
    live.apply({
      kind: "bundle",
      bundle_id: "xyz",
      tip_lamports: 1000,
      tip_account: "Tip1",
      region: "ny",
      signatures: [],
      phase: "submitted",
      ts: "2026-06-09T00:00:00Z",
    });
    live.apply({ kind: "failure", id: "xyz", classification: { class: "expired_blockhash", confidence: 0.92 } });
    const b = live.snapshot().bundles[0]!;
    expect(b.failure_class).toBe("expired_blockhash");
    expect(b.stage).toBe("failed");
  });

  it("reports freshness against the injected clock", () => {
    const c = clock(10_000);
    const live = new LiveTelemetry(c.now);
    expect(live.hasData()).toBe(false); // nothing yet
    live.apply({ kind: "slot", slot: 1, parent: null, status: "processed", ts: "2026-06-09T00:00:00Z" });
    expect(live.hasData()).toBe(true);
    c.set(10_000 + 20_000); // 20s later — stale
    expect(live.hasData()).toBe(false);
  });
});

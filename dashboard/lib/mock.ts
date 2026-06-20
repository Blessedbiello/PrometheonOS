/**
 * Deterministic mock telemetry generator. Holds rolling state across `tick()` calls so the
 * dashboard renders a believable evolving stream. Replaced by real NATS-driven snapshots once the
 * core integration lands.
 */
import type {
  BundleRow,
  DashboardSnapshot,
  DecisionRow,
  HealthSnapshot,
  SlotRow,
} from "./types.js";

const LEADERS = [
  { id: "JitoVldA1Bz9P8VqHsXn9TKkqPGm7zKp", jito: true },
  { id: "AnzaVldB2Cq7RfTGkUvZ8hWzKnNcEm3F", jito: false },
  { id: "JitoVldC3DwUyJpMsXcQ5gBhKfYvLqRt", jito: true },
  { id: "AnzaVldD4Eka6PnQbHfLmZjGtNcVkXdR", jito: false },
];

const TIP_ACCOUNTS = [
  "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
  "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
];

/** Seedable deterministic PRNG (mulberry32). */
function rng(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const SLOTS_WINDOW = 20;
const BUNDLES_WINDOW = 8;
const DECISIONS_WINDOW = 10;

export class MockState {
  private rand: () => number;
  private slot = 298_430_000;
  private startTs: number;
  private bundleSeq = 0;
  private decisionSeq = 0;
  public slots: SlotRow[] = [];
  public bundles: BundleRow[] = [];
  public decisions: DecisionRow[] = [];
  /** Rolling counters that drive the health snapshot. */
  private submitted = 0;
  private landed = 0;
  private totalTipLamports = 0;
  private totalCostLamports = 0;
  private retriesTotal = 0;
  private retriesLanded = 0;
  private skipped = 0;
  private produced = 0;
  private confirmLatencies: number[] = [];

  constructor(seed = 1) {
    this.rand = rng(seed);
    this.startTs = Date.UTC(2026, 4, 28, 0, 0, 0);
  }

  /** Advance one tick (≈ one slot, ~400ms). Mutates state and returns the snapshot. */
  tick(): DashboardSnapshot {
    this.slot += 1;
    const skippedThisSlot = this.rand() < 0.08; // ~8% skip rate
    if (skippedThisSlot) {
      this.skipped += 1;
    } else {
      this.produced += 1;
      this.pushSlot();
    }
    this.advanceBundles();
    this.maybeStartBundle();
    this.maybeEmitDecision();

    return this.snapshot();
  }

  private pushSlot(): void {
    const leader = LEADERS[Math.floor(this.rand() * LEADERS.length)]!;
    const row: SlotRow = {
      slot: this.slot,
      parent: this.slots.at(-1)?.slot ?? this.slot - 1,
      status: this.pickSlotStatus(),
      ts: this.ts(),
      leader: leader.id,
      jito: leader.jito,
      skipped: [],
    };
    this.slots.push(row);
    if (this.slots.length > SLOTS_WINDOW) this.slots.shift();
  }

  private pickSlotStatus(): SlotRow["status"] {
    // Most observed slots are processed → confirmed → finalized over time. For a snapshot view,
    // we pick a representative status weighted toward "confirmed" (the common display state).
    const r = this.rand();
    if (r < 0.10) return "processed";
    if (r < 0.85) return "confirmed";
    return "finalized";
  }

  private advanceBundles(): void {
    for (const b of this.bundles) {
      if (b.stage === "submitted" && this.rand() < 0.6) {
        b.stage = "processed";
        b.slot = this.slot;
        b.latencies.processed_ms = this.msSince(b.submit_ts);
      } else if (b.stage === "processed" && this.rand() < 0.7) {
        b.stage = "confirmed";
        b.latencies.confirmed_ms = this.msSince(b.submit_ts);
        this.confirmLatencies.push(b.latencies.confirmed_ms);
        if (this.confirmLatencies.length > 32) this.confirmLatencies.shift();
      } else if (b.stage === "confirmed" && this.rand() < 0.5) {
        b.stage = "finalized";
        b.latencies.finalized_ms = this.msSince(b.submit_ts);
        this.landed += 1;
        this.totalTipLamports += b.tip_lamports;
        this.totalCostLamports += b.tip_lamports + 5000; // tip + base fee
      } else if (b.stage === "submitted" && this.rand() < 0.05) {
        // ~5% of submitted bundles fail (mix of injected expiry and organic).
        b.stage = "expired";
        b.failure_class = "expired_blockhash";
        b.injected = this.rand() < 0.5;
      }
    }
    // Trim ancient bundles.
    while (this.bundles.length > BUNDLES_WINDOW) this.bundles.shift();
  }

  private maybeStartBundle(): void {
    if (this.rand() < 0.35) {
      this.bundleSeq += 1;
      this.submitted += 1;
      this.bundles.push({
        bundle_id: `bnd_${this.bundleSeq.toString(16).padStart(6, "0")}${randomHex(this.rand, 6)}`,
        tip_lamports: 8_000 + Math.floor(this.rand() * 20_000),
        tip_account: TIP_ACCOUNTS[Math.floor(this.rand() * TIP_ACCOUNTS.length)]!,
        region: "ny",
        signatures: [randomHex(this.rand, 16)],
        stage: "submitted",
        slot: null,
        submit_ts: this.ts(),
        latencies: { processed_ms: null, confirmed_ms: null, finalized_ms: null },
        failure_class: null,
        injected: false,
        retry_attempt: 0,
      });
    }
  }

  private maybeEmitDecision(): void {
    if (this.rand() > 0.12) return;
    const r = this.rand();
    let row: DecisionRow;
    if (r < 0.45) {
      const before = 12_000;
      const congestion = 0.5 + this.rand() * 0.45;
      const after = Math.round(before * (1 + 0.5 * congestion));
      row = {
        decision_type: "tip",
        action: `tip ${before}→${after} lamports`,
        reasoning: `Tip increased: 50th-pct floor ${Math.round(before * 1.1)} lamports, congestion ${congestion.toFixed(2)} rising, last 2 bundles at ${before} did not land.`,
        confidence: 0.7 + this.rand() * 0.25,
        inputs_considered: { tipFloorP50Lamports: before * 1.1, congestionScore: congestion },
        before: { tip: before },
        after: { tip: after },
        provider: "anthropic",
        latency_ms: 800 + Math.floor(this.rand() * 1500),
        ts: this.ts(),
      };
    } else if (r < 0.75) {
      row = {
        decision_type: "retry",
        action: "refresh blockhash, recalc tip, resubmit",
        reasoning: "Classified blockhash_expiry conf 0.92; blockhash age > 150 blocks; refresh at confirmed, bump tip 14k→17k (congestion up since submit).",
        confidence: 0.85 + this.rand() * 0.1,
        inputs_considered: { failure_class: "expired_blockhash", confidence: 0.92, attempt: 2 },
        before: { blockhash: "stale", tip: 14_000 },
        after: { blockhash: "fresh", tip: 17_000 },
        provider: "anthropic",
        latency_ms: 900 + Math.floor(this.rand() * 1200),
        ts: this.ts(),
      };
      this.retriesTotal += 1;
      if (this.rand() < 0.83) this.retriesLanded += 1;
    } else {
      row = {
        decision_type: "timing",
        action: "hold 2 slots",
        reasoning: "Slot stability 0.42 (elevated skip risk); next Jito leader window in 3 slots — wait for healthier window.",
        confidence: 0.6 + this.rand() * 0.2,
        inputs_considered: { slot_stability_score: 0.42, next_jito_leader_in: 3 },
        before: { policy: "submit_now" },
        after: { policy: "hold_2_slots" },
        provider: "anthropic",
        latency_ms: 700 + Math.floor(this.rand() * 800),
        ts: this.ts(),
      };
    }
    this.decisions.unshift(row);
    if (this.decisions.length > DECISIONS_WINDOW) this.decisions.length = DECISIONS_WINDOW;
    this.decisionSeq += 1;
  }

  private snapshot(): DashboardSnapshot {
    const health = this.computeHealth();
    const nextJito = this.slots
      .slice()
      .reverse()
      .findIndex((s) => s.jito);
    return {
      ts: this.ts(),
      source: "mock",
      network: "mock",
      current_slot: this.slot,
      next_jito_leader_in_slots: nextJito >= 0 ? nextJito : null,
      ai_provider: "anthropic",
      slots: [...this.slots].reverse(), // newest first for the panel
      bundles: [...this.bundles],
      decisions: [...this.decisions],
      health,
    };
  }

  private computeHealth(): HealthSnapshot {
    const total = this.produced + this.skipped;
    const slot_stability_score = total === 0 ? 1 : this.produced / total;
    const skip_rate = total === 0 ? 0 : this.skipped / total;
    const avg_confirmed = this.confirmLatencies.length
      ? this.confirmLatencies.reduce((a, b) => a + b, 0) / this.confirmLatencies.length
      : null;
    const tip_floor = 12_000 + Math.round(this.rand() * 2_000);
    const congestion = clamp(
      0.4 * skip_rate + 0.35 * ((avg_confirmed ?? 0) / 3000) + 0.25 * (tip_floor / 1_000_000),
      0,
      1,
    );
    const landed = this.landed;
    const submitted = this.submitted;
    const totalTip = this.totalTipLamports;
    const totalCost = this.totalCostLamports;
    return {
      ts: this.ts(),
      congestion_score: congestion,
      slot_stability_score,
      bundle_landing_probability: submitted === 0 ? 0 : landed / submitted,
      retry_success_rate: this.retriesTotal === 0 ? 0 : this.retriesLanded / this.retriesTotal,
      tip_efficiency_ratio: totalTip === 0 ? 0 : landed / totalTip,
      cost_per_successful_landing: landed === 0 ? null : totalCost / landed,
      avg_confirmed_latency_ms: avg_confirmed,
      confirm_latency_variance_ms: variance(this.confirmLatencies),
      tip_floor_lamports: tip_floor,
      processed_to_confirmed_delta_ms: avg_confirmed ? Math.max(50, avg_confirmed * 0.7) : null,
    };
  }

  private ts(): string {
    return new Date(this.startTs + this.slot * 400).toISOString();
  }

  private msSince(iso: string): number {
    return new Date(this.ts()).getTime() - new Date(iso).getTime();
  }
}

function clamp(x: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, x));
}

function variance(xs: number[]): number | null {
  if (xs.length === 0) return null;
  const m = xs.reduce((a, b) => a + b, 0) / xs.length;
  return xs.reduce((acc, v) => acc + (v - m) ** 2, 0) / xs.length;
}

function randomHex(rand: () => number, n: number): string {
  let out = "";
  for (let i = 0; i < n; i++) out += Math.floor(rand() * 16).toString(16);
  return out;
}

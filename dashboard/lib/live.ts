/**
 * Live telemetry bridge: NATS → DashboardSnapshot.
 *
 * A process-singleton subscribes to the engine's `telemetry.>` and `decision.>` subjects and folds
 * each event into a rolling `DashboardSnapshot`, which the `/api/telemetry` route serves to the
 * polling UI. The reducer ([`LiveTelemetry.apply`]) is pure and unit-tested; the NATS connection is
 * thin I/O created lazily on first request.
 *
 * Events are the Rust `TelemetryEvent` JSON (internally tagged by `kind`): `slot`, `health`,
 * `bundle`, `lifecycle`, `failure`, `decision`.
 */
import { connect, type NatsConnection, type Subscription } from "nats";
import type {
  BundleRow,
  DashboardSnapshot,
  DecisionRow,
  FailureClass,
  HealthSnapshot,
  LifecycleStage,
  SlotRow,
} from "./types.js";

const SLOTS_WINDOW = 20;
const DECISIONS_WINDOW = 12;
const BUNDLES_WINDOW = 10;
/** Consider the feed "live" only if an event arrived within this window. */
const FRESH_MS = 15_000;

const EMPTY_HEALTH: HealthSnapshot = {
  ts: new Date(0).toISOString(),
  congestion_score: 0,
  slot_stability_score: 1,
  bundle_landing_probability: 0,
  retry_success_rate: 0,
  tip_efficiency_ratio: 0,
  cost_per_successful_landing: null,
  avg_confirmed_latency_ms: null,
  confirm_latency_variance_ms: null,
  tip_floor_lamports: 0,
  processed_to_confirmed_delta_ms: null,
};

const FAILURE_STAGES: ReadonlySet<string> = new Set(["failed", "expired", "dropped"]);

export class LiveTelemetry {
  private slots: SlotRow[] = [];
  private decisions: DecisionRow[] = [];
  private bundles = new Map<string, BundleRow>();
  private health: HealthSnapshot = EMPTY_HEALTH;
  private currentSlot = 0;
  private aiProvider = "—";
  private network: DashboardSnapshot["network"] = "mainnet";
  private lastEventAt = 0;

  /** Inject a clock for deterministic tests. */
  constructor(private now: () => number = () => Date.now()) {}

  /** Fold one telemetry event into the rolling state. Pure aside from the freshness clock. */
  apply(ev: Record<string, unknown>): void {
    const kind = ev.kind as string | undefined;
    if (!kind) return;
    this.lastEventAt = this.now();

    switch (kind) {
      case "slot": {
        const slot = Number(ev.slot);
        const row: SlotRow = {
          slot,
          parent: ev.parent == null ? null : Number(ev.parent),
          status: ev.status as SlotRow["status"],
          ts: String(ev.ts),
          leader: null,
          jito: false,
          skipped: [],
        };
        if (slot > this.currentSlot) this.currentSlot = slot;
        this.slots.push(row);
        if (this.slots.length > SLOTS_WINDOW) this.slots.shift();
        break;
      }
      case "health": {
        this.health = {
          ...EMPTY_HEALTH,
          ...(ev as unknown as HealthSnapshot),
          processed_to_confirmed_delta_ms:
            (ev.processed_to_confirmed_delta_ms as number | null) ?? null,
        };
        break;
      }
      case "decision": {
        const row: DecisionRow = {
          decision_type: ev.decision_type as DecisionRow["decision_type"],
          action: String(ev.action ?? ""),
          reasoning: String(ev.reasoning ?? ""),
          confidence: Number(ev.confidence ?? 0),
          inputs_considered: (ev.inputs_considered as Record<string, unknown>) ?? {},
          before: (ev.before as Record<string, unknown> | null) ?? null,
          after: (ev.after as Record<string, unknown> | null) ?? null,
          provider: String(ev.provider ?? "—"),
          latency_ms: Number(ev.latency_ms ?? 0),
          ts: String(ev.ts),
        };
        this.aiProvider = row.provider;
        this.decisions.push(row);
        if (this.decisions.length > DECISIONS_WINDOW) this.decisions.shift();
        break;
      }
      case "bundle": {
        const id = String(ev.bundle_id);
        const existing = this.bundles.get(id);
        const row: BundleRow = existing ?? {
          bundle_id: id,
          tip_lamports: Number(ev.tip_lamports ?? 0),
          tip_account: String(ev.tip_account ?? ""),
          region: String(ev.region ?? ""),
          signatures: (ev.signatures as string[]) ?? [],
          stage: "submitted",
          slot: null,
          submit_ts: String(ev.ts),
          latencies: { processed_ms: null, confirmed_ms: null, finalized_ms: null },
          failure_class: null,
          failure_confidence: null,
          injected: false,
          retry_attempt: ev.attempt != null ? Math.max(0, Number(ev.attempt) - 1) : 0,
          base_id: (ev.base_id as string | undefined) ?? null,
          attempt: ev.attempt != null ? Number(ev.attempt) : null,
        };
        row.tip_lamports = Number(ev.tip_lamports ?? row.tip_lamports);
        this.upsertBundle(row);
        break;
      }
      case "lifecycle": {
        const id = String(ev.id);
        const event = (ev.event ?? {}) as Record<string, unknown>;
        const stage = event.stage as LifecycleStage | undefined;
        const row = this.bundles.get(id);
        if (!row || !stage) break;
        row.stage = stage;
        if (event.slot != null) row.slot = Number(event.slot);
        const ts = String(event.ts ?? row.submit_ts);
        const delta = new Date(ts).getTime() - new Date(row.submit_ts).getTime();
        if (stage === "processed") row.latencies.processed_ms = delta;
        if (stage === "confirmed") row.latencies.confirmed_ms = delta;
        if (stage === "finalized") row.latencies.finalized_ms = delta;
        this.upsertBundle(row);
        break;
      }
      case "failure": {
        const id = String(ev.id);
        const cls = (ev.classification ?? {}) as Record<string, unknown>;
        const row = this.bundles.get(id);
        if (!row) break;
        row.failure_class = (cls.class as FailureClass) ?? "unclassified";
        row.failure_confidence = cls.confidence != null ? Number(cls.confidence) : null;
        if (!FAILURE_STAGES.has(row.stage)) row.stage = "failed";
        this.upsertBundle(row);
        break;
      }
      default:
        break;
    }
  }

  private upsertBundle(row: BundleRow): void {
    this.bundles.set(row.bundle_id, row);
    // Bound the map by insertion order (Map preserves it): drop the oldest.
    while (this.bundles.size > BUNDLES_WINDOW) {
      const oldest = this.bundles.keys().next().value as string | undefined;
      if (oldest === undefined) break;
      this.bundles.delete(oldest);
    }
  }

  /** True if connected and an event arrived recently (so the route can fall back to mock). */
  hasData(): boolean {
    return this.lastEventAt > 0 && this.now() - this.lastEventAt < FRESH_MS;
  }

  snapshot(): DashboardSnapshot {
    return {
      ts: new Date(this.now()).toISOString(),
      source: "live",
      network: this.network,
      current_slot: this.currentSlot,
      next_jito_leader_in_slots: null,
      ai_provider: this.aiProvider,
      slots: this.slots.slice(),
      bundles: [...this.bundles.values()],
      decisions: this.decisions.slice(),
      health: this.health,
    };
  }
}

// ── Process singleton + lazy NATS connection ────────────────────────────────────────────────────

let singleton: LiveTelemetry | null = null;
let nc: NatsConnection | null = null;
let connecting: Promise<void> | null = null;

export function liveTelemetry(): LiveTelemetry {
  if (!singleton) singleton = new LiveTelemetry();
  return singleton;
}

/** Connect once and start folding events. Safe to call on every request (no-op once connected). */
export async function ensureConnected(url: string): Promise<boolean> {
  const live = liveTelemetry();
  if (nc) return true;
  if (!connecting) {
    connecting = (async () => {
      const conn = await connect({
        servers: url,
        name: "prometheon-dashboard",
        maxReconnectAttempts: -1,
      });
      nc = conn;
      void consume(conn.subscribe("telemetry.>"), live);
      void consume(conn.subscribe("decision.>"), live);
    })().catch((err) => {
      connecting = null; // allow a later retry
      throw err;
    });
  }
  try {
    await connecting;
    return true;
  } catch {
    return false;
  }
}

async function consume(sub: Subscription, live: LiveTelemetry): Promise<void> {
  for await (const msg of sub) {
    try {
      live.apply(JSON.parse(msg.string()) as Record<string, unknown>);
    } catch {
      // ignore malformed events; telemetry must never crash the dashboard
    }
  }
}

/**
 * TypeScript shapes mirroring the Rust telemetry contract.
 *
 * These will be replaced by types generated from `contracts/json-schema/` once the schema-gen
 * pipeline is wired (Phase 4 follow-up). For now they are hand-aligned with `prometheon-telemetry`.
 */

export type LifecycleStage =
  | "submitted"
  | "processed"
  | "confirmed"
  | "finalized"
  | "failed"
  | "expired"
  | "dropped";

export type SlotStatus =
  | "first_shred_received"
  | "created_bank"
  | "completed"
  | "processed"
  | "confirmed"
  | "finalized"
  | "dead";

export type DecisionType = "tip" | "timing" | "retry";

export type FailureClass =
  | "expired_blockhash"
  | "fee_too_low"
  | "compute_exceeded"
  | "bundle_failure"
  | "leader_miss"
  | "skipped_slot"
  | "duplicate_signature"
  | "instruction_error"
  | "confirmation_timeout"
  | "unclassified";

export interface SlotRow {
  slot: number;
  parent: number | null;
  status: SlotStatus;
  ts: string;
  leader: string | null;
  /** Is the slot's leader a Jito-Solana validator (and therefore honoring bundles)? */
  jito: boolean;
  /** Skipped slots between parent+1 and slot, derived by the ingest tracker. */
  skipped: number[];
}

export interface BundleLatencies {
  processed_ms: number | null;
  confirmed_ms: number | null;
  finalized_ms: number | null;
}

export interface BundleRow {
  bundle_id: string;
  tip_lamports: number;
  tip_account: string;
  region: string;
  signatures: string[];
  stage: LifecycleStage;
  slot: number | null;
  submit_ts: string;
  latencies: BundleLatencies;
  /** Failure class if stage is failed/expired/dropped, plus injection flag. */
  failure_class: FailureClass | null;
  /** Classifier confidence for `failure_class` (0–1), if classified. */
  failure_confidence: number | null;
  injected: boolean;
  retry_attempt: number;
  /** Recovery linkage: retries of one logical bundle share a `base_id`; `attempt` is 1-indexed. The
   *  Execution Rail threads a failed attempt to its recovered resubmission by these. */
  base_id: string | null;
  attempt: number | null;
}

export interface DecisionRow {
  decision_type: DecisionType;
  action: string;
  reasoning: string;
  confidence: number;
  inputs_considered: Record<string, unknown>;
  before: Record<string, unknown> | null;
  after: Record<string, unknown> | null;
  provider: string;
  latency_ms: number;
  ts: string;
}

export interface HealthSnapshot {
  ts: string;
  congestion_score: number;
  slot_stability_score: number;
  bundle_landing_probability: number;
  retry_success_rate: number;
  tip_efficiency_ratio: number;
  cost_per_successful_landing: number | null;
  avg_confirmed_latency_ms: number | null;
  confirm_latency_variance_ms: number | null;
  tip_floor_lamports: number;
  /** Live Jito tip-floor distribution (P75/P95) when available — the competitive band the AI targets.
   *  Optional: the live engine currently emits only the median; the proof-replay carries all three. */
  tip_floor_p75_lamports?: number | null;
  tip_floor_p95_lamports?: number | null;
  /** processed→confirmed delta (network-health signal behind README Q1). */
  processed_to_confirmed_delta_ms: number | null;
}

export interface DashboardSnapshot {
  ts: string;
  /** Provenance of this snapshot — the UI must surface it honestly and never show "live" over
   *  non-live data. `live` = real engine telemetry over NATS; `mock` = the offline simulation;
   *  `proof` = a deterministic replay of the COMMITTED mainnet proof run (real on-chain data + real
   *  explorer links), used so the recovery hero plays on cue without faking liveness. */
  source: "live" | "mock" | "proof";
  network: "testnet" | "mainnet" | "mock";
  current_slot: number;
  next_jito_leader_in_slots: number | null;
  ai_provider: string;
  slots: SlotRow[];
  bundles: BundleRow[];
  decisions: DecisionRow[];
  health: HealthSnapshot;
}

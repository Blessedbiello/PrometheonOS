# Telemetry & Schema Architecture

**Single source of truth:** Rust types in `prometheon-types` derive `serde` + `schemars`. A build
step emits JSON Schema into `contracts/`; TS types are generated from those schemas and validated
at runtime with zod. CI fails on schema drift. NATS messages are JSON, validated on both sides.

## NATS subjects (planned)
- `telemetry.slot` — slot status updates (FirstShredReceived…Finalized, Dead)
- `telemetry.leader` — leader-schedule window updates
- `telemetry.bundle` — submit/ack/status transitions
- `telemetry.lifecycle` — per-tx Submitted/Processed/Confirmed/Finalized + deltas
- `telemetry.failure` — classified failures + confidence
- `telemetry.health` — network-health + execution-quality snapshots
- `decision.request.<type>` — core → agent decision requests (request/reply)
- `decision.<type>` — agent → core/dashboard decisions + reasoning traces

## Core event types (to be defined in `prometheon-types`, Phase 4)
- `SlotEvent { slot, parent, status, ts }`
- `BundleEvent { bundle_id, tip_lamports, tip_account, region, sigs[], ts, phase }`
- `LifecycleEvent { sig, stage, slot, ts, delta_ms_from_prev }`
- `FailureEvent { sig|bundle_id, class, confidence, signals, ts }`
- `HealthSnapshot { congestion_score, slot_stability_score, leader_reliability_score,
   bundle_landing_probability, expiry_risk_score, processed/confirmed/finalized_latency_ms, ts }`
- `Decision { type, inputs_considered, reasoning, confidence, action, before, after, provider, latency_ms, ts }`

## Execution-quality metrics (computed in `prometheon-netmodel`, Phase 4)
See the metric table in the plan; definitions land here with formulas + window sizes.

## Persistence (Postgres + TimescaleDB)
Hypertables for time-series events; tables for bundles, lifecycle, failures, decisions, metrics.
Schema DDL added in Phase 4.

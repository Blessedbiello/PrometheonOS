# PrometheonOS — Architecture Design Document

> Source of the public architecture document (mirrored to a public Notion/Google Docs URL for
> submission). Living document; sections marked _(pending)_ are filled as their phase lands.

## 1. Executive summary

PrometheonOS is an autonomous Solana execution-intelligence engine. It streams live slot/leader
data from Yellowstone gRPC, constructs and submits Jito bundles with dynamically computed tips,
tracks each transaction across `Submitted → Processed → Confirmed → Finalized`, classifies
failures, and lets an AI strategist make and explain real operational decisions (tip sizing,
submission timing, autonomous retry). Landing is confirmed from the stream; RPC is a cross-check.

## 2. System goals

- Real protocol fidelity (no hardcoded shortcuts; correct commitment handling).
- Everything observable, timestamped, measurable; every failure classified; every retry and AI
  decision justified.
- Clean separation between the deterministic core and the AI layer.
- Production-grade reliability: reconnection, backpressure, graceful degradation.

## 3. Design philosophy — the AI is a strategist, not in the hot path

An LLM call (~0.5–3 s) cannot sit inside the leader-window catch loop. The Rust core is the
**deterministic hot path** (catch leader window, build/submit bundles, track lifecycle) and runs
on the *latest policy* the agent has set. The TS AI agent is an **asynchronous strategist**: it
reacts to telemetry (health snapshots, failure/retry events) to set policy (tip target, hold/go)
and to reason about discrete, non-microsecond-critical events (failure classification → retry).

## 4. Solana transaction lifecycle deep dive

Commitment: `processed` (in a block, no votes, fork-revertible, ~400–600 ms) → `confirmed`
(≥⅔ stake optimistic vote, ~1–2 s) → `finalized` (32-slot lockout, rooted, ~12.8 s). Blockhash
valid 150 blocks (~60–90 s); `lastValidBlockHeight = current + 150`; expiry is measured by
**block height** (skipped slots don't burn the budget). TPU pipeline: fetch (QUIC) → sigverify
(dedup) → banking (+PoH) → broadcast (Turbine). Streaming beats polling because polling only
surfaces a tx after replay + vote + RPC indexing. _(expanded with our observations in Phase 8)_

## 5. Architecture diagram

System context + data-flow diagram in [`DIAGRAMS.md` §1](DIAGRAMS.md). Rust core ⇄ NATS ⇄ {TS AI
agent, Next.js dashboard}; Postgres+TimescaleDB persistence; Prometheus metrics.

## 6. Component breakdown
- **ingest** — Yellowstone slots/leader/tx; reconnect (`from_slot`), backpressure, gap detect.
- **bundle** — tip-floor client, `getTipAccounts` cache, bundle build, `sendBundle`, statuses.
- **lifecycle** — stream-driven state machine + latency deltas.
- **failure** — classifier + taxonomy + confidence.
- **retry** — orchestrator state machine; blockhash refresh; tip recalc; backoff.
- **netmodel** — network-health + execution-quality metrics.
- **telemetry** — typed events → NATS + Postgres + Prometheus.
- **faultinject** — chaos scenarios.
- **ai-agent (TS)** — pluggable LLM provider; tip/timing/retry decisions; reasoning traces.
- **dashboard (Next.js)** — live slots/leaders, bundles, lifecycle, retries, AI timeline, health.

## 7. Event flow
Single-bundle event timeline in [`DIAGRAMS.md` §5](DIAGRAMS.md); lifecycle state machine in §2.
_(annotated with real timings in Phase 8.)_
## 8. AI decision pipeline — sequence in [`DIAGRAMS.md` §4](DIAGRAMS.md); design in §AI agent (Phase 5).
## 9. Failure handling matrix — see [`FAILURE-TAXONOMY.md`](FAILURE-TAXONOMY.md) (full 18-class, observable-vs-inferred).
## 10. Telemetry architecture — see [`TELEMETRY-SCHEMA.md`](TELEMETRY-SCHEMA.md)
## 11. Retry state machine — see [`DIAGRAMS.md` §3](DIAGRAMS.md) _(impl Phase 6; RFC 0003)_
## 12. Jito integration strategy
Block Engine JSON-RPC (`/api/v1/bundles`, `getInflightBundleStatuses`, `getBundleStatuses`,
`getTipAccounts`); ≤5 txs atomic/same-block; mandatory tip = real transfer to one of 8 tip
accounts, co-located in the strategy tx, never in an ALT; dynamic tip from
`bundles.jito.wtf/.../tip_floor`; rate limit ~1 rps/IP/region (UUID for more); region failover.
## 13. Yellowstone stream design
Bidirectional `Subscribe`; named filter maps (`slots`, `transactions`/`transactions_status`,
`accounts`, `blocks`...); request-global commitment; `from_slot` replay (~1000-slot buffer);
server `Ping`→client `ping{id}` keepalive; bounded-channel + worker-pool backpressure; zstd; raise
max decode size.
## 14. Commitment tracking logic
A per-submission state machine (`prometheon-lifecycle`) advances `Submitted → Processed → Confirmed
→ Finalized` (with `Failed`/`Expired`/`Dropped` branches), capturing the slot, timestamp, and
inter-stage delta at each transition; illegal transitions are rejected so the recorded history is
always a valid path. It is driven by the **stream**: `prometheon-core::proof::PendingBundles`
correlates our submitted signatures to the right lifecycle — a tx-status event marks `Processed`
(capturing the landed slot), and that slot's later `Confirmed`/`Finalized` slot-status events
advance it. RPC (`getBlockHeight`/`isBlockhashValid`, `getBundleStatuses`) is only a cross-check.
The `processed→confirmed` delta is surfaced as a consensus-health signal (README Q1).
## 15. Performance considerations
Per-component latency sensitivity:
- **ingest** (high) — must keep pace with the tip; backpressure via bounded channel + worker pool;
  zstd above ~7ms RTT; raise gRPC max decode size; co-locate near the Yellowstone region.
- **bundle/submit** (high) — leader-window-bound; region-closest Block Engine; pace under the
  ~1 rps limit (UUID for more); tip co-located to avoid paying on failure.
- **lifecycle/failure/netmodel** (medium) — event-driven off the ingest channel, not on the wire.
- **AI agent** (low) — async strategist; hot path runs on last cached policy, never blocks on it.
The deterministic Rust hot path avoids GC jitter (RFC 0002 D1); the LLM is deliberately out of it
(§3). Latency-sensitive assumptions carry **V** flags and are validated against live infra in Phase 1–2.

## 16. Scalability considerations
Single-engine scope is sufficient for the bounty, but the seams scale: NATS decouples producers
from consumers; Timescale hypertables partition time-series; the ingest worker pool scales with
cores; multiple Block Engine regions allow submission fan-out. Bottleneck is client-side processing,
not the wire — mitigated by the ingest/processing split (RFC 0001).
## 17. Fault injection methodology — see [`EXPERIMENTS.md`](EXPERIMENTS.md)
## 18. Security considerations
No secrets in repo; keypairs gitignored; minimal mainnet funds; tip co-location prevents paying
on failed bundles; pre/post account checks guard against uncled-block rebroadcast.
## 19. Cost analysis _(pending — actuals from the mainnet proof run)_
## 20. Lessons learned _(pending — Phase 8)_

## 21. Implementation status & live validation
The engine is wired end-to-end and validated against the **live SolInfra mainnet stream** (gRPC
`fra.grpc.solinfra.dev:443`), not just unit-tested in isolation:

- **Ingestion → health → sinks.** `prometheon-core::engine` streams Yellowstone slots into the
  `NetworkHealthModel` and fans every `TelemetryEvent` through one `emit`: NATS pub/sub, a
  Postgres/TimescaleDB hypertable (`telemetry_event` + `v_decision`/`v_bundle`/`v_lifecycle`/
  `v_failure` projection views), and a Prometheus `/metrics` exporter. Validated live: slots
  streaming, congestion reacting to a real leader skip (stability `1.0 → 0.889`), 60 events on the
  bus, rows in Postgres, `prometheon_*` gauges served.
- **AI in the loop — autonomous retry with fault injection (`prometheon-core::saga`).** The agent
  owns the **retry** decision end-to-end during the run: on a non-landing the core `classify`s the
  failure, requests a retry decision over NATS (`decision.request.retry`), and the agent reasons about
  refresh + re-price; the core enforces safety (expiry **always** forces a blockhash refresh; the tip
  is clamped) and resubmits the next attempt — which lands. The agent also sets the **tip** per bundle
  (`decision.request.tip`) and makes a **submission-timing** call from the live leader schedule. Every
  decision is emitted as `Decision` telemetry → dashboard timeline + the exported log. The
  deterministic `prometheon-retry` policy is the safety fallback. The whole AI-driven loop, including
  recovery (attempt 1 failed → attempt 2 landed), is regression-tested with no network in
  `tests/saga_pipeline.rs`. Two small traits (`DecisionSource`, `Submitter`) keep it testable; the
  live binary plugs in the NATS bus and the real submitter.
- **Leader-window detection (`prometheon-core::leader`).** The upcoming leader schedule from Solana
  RPC `getSlotLeaders` (`rpc::get_slot_leaders`) yields the current leader + slots-until-rotation,
  feeding the submission-timing decision. The Jito searcher `getNextScheduledLeader` (which also says
  *which* upcoming leaders run Jito) is a gRPC searcher-API method requiring approved auth — its HTTP
  form 404s — so we time against the RPC schedule and rely on the Block Engine routing the bundle to
  the next Jito leader; the searcher-gRPC path is a documented optional enhancement.
- **Submit path.** `prometheon-core::proof` assembles a real bundle from live data (fresh blockhash,
  rotating tip account, live-floor tip clamped to policy bounds, congestion-scaled CU price), signs
  it, and either simulates (free dry-run) or submits it. Dry-run validated on mainnet: a dynamic
  3329-lamport tip, 4 rotating tip accounts, distinct blockhashes/signatures; the simulator returns
  `AccountNotFound`, i.e. assembled correctly — only funding gates broadcast.
- **Submit → telemetry → export pipeline.** The saga (and the simpler `proof_run`) drive all in-flight
  bundles over **one shared** Yellowstone stream and emit `Bundle`/`Lifecycle`/`Failure`/`Decision`
  events through the same `Sinks` (NATS + Postgres) the engine uses, so `export-log` reads what the run
  persists — producing the per-bundle lifecycle table **and an AI Decision Timeline**
  (`export::render_decisions_markdown`). The path is integration-tested without a network
  (`tests/proof_pipeline.rs`, `tests/saga_pipeline.rs`). Fault injection
  (`--inject low-tip,stale-blockhash`) guarantees the bounty's ≥2 failure cases.
- **One contract.** Rust telemetry types (`schemars`) generate `contracts/json-schema/*` and the TS
  types; CI fails on drift. The dashboard consumes the live NATS feed and labels its status honestly
  (`source: live|mock` → "live"/"simulated"), never showing a live indicator over the mock feed.

Remaining (Tier 5, gated on funding): the funded mainnet proof run (`scripts/run-proof.sh`) to
produce the explorer-verifiable lifecycle log with real injected failures and a real AI reasoning
trace, plus §19/§20 actuals + lessons from it.

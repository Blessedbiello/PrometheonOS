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
## 14. Commitment tracking logic _(pending — Phase 3)_
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

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
**Fund safety.** No secrets in repo (real `.env` + `wallets/` gitignored; verified clean across git
history); keypairs load with length validation and are never logged/cloned/serialized. The only
value-moving instructions the wallet signs are a self-transfer (payer→payer) and the tip
(payer→a Jito `getTipAccounts` address) — no attacker-controllable recipient, no drain/double-spend.
Every tip is **clamped to a competitive `[200_000, 1_000_000]`-lamport band (≤0.001 SOL) before
signing**: the `200_000` lower bound is a **competitive floor** that lifts a sub-floor AI proposal so
the bundle still wins inclusion, and the `1_000_000` cap means a poisoned AI decision or manipulated
telemetry can overpay by at most ~0.001 SOL/bundle. The tip is **co-located** in the same tx as the
strategy ix, so a non-landing pays nothing. The AI proposes a tip *value* within these fixed,
code-enforced bounds it cannot widen — and in practice (see the committed proof run) the **floor, not
the model's exact number, sets the tip whenever the AI under-prices**: the deterministic competitive
floor is doing the landing work, by design.

**Known limitations (disclosed).** (1) **Retry-without-cancel** — Solana has no bundle cancel, so a
given-up attempt and its retry can both land (two self-transfers + two clamped tips for one logical
bundle); the economic exposure is bounded by the clamp, and a durable-nonce single-flight scheme is
future work. (2) **Infra is local-only** — `docker-compose` binds all ports to `127.0.0.1` with dev
placeholder credentials; NATS/Postgres/Prometheus/Grafana and the engine `/metrics` (default
`127.0.0.1:9100`) must sit behind a VPN/firewall for any remote demo. NATS auth is supported by
embedding credentials in `NATS_URL` (`nats://user:pass@host`); enable it for non-local deployments.
(3) The dashboard `/api/telemetry` route is unauthenticated and intended for local/same-origin use.
## 19. Cost analysis
The committed mainnet proof run (14 submissions: 12 landed + 2 AI-recovered) cost **~0.0025 SOL total**
(~$0.5 at the time) — competitive tips of 200,000–235,000 lamports on landed bundles plus base fees; the
two injected non-landings paid **zero tip** (co-located tip). The dominant cost lever is the
**competitive tip floor** (§ tip policy), not the priority fee; a healthier live floor would let the
AI's own (lower) proposals land and reduce this.

## 20. Lessons learned
- **The landed-tip distribution is brutally skewed.** The Jito `tip_floor` P50 routinely collapses to
  the ~1000-lamport noise floor, so a P50-anchored tip almost never wins inclusion (in our first attempts
  nothing landed until we moved to the P75–P95 band).
  Only the P75–P95 band lands; we enforce a deterministic competitive floor (200k) so a sub-floor AI
  proposal still lands — which means, honestly, the floor (not the model's number) sets most tips.
- **Probe-time signals are confounded by the give-up wait.** We only probe a non-land after the full
  window, by which point the blockhash has naturally expired for *every* non-land — so a sub-floor tip
  (time-invariant) must outrank a probe-time expiry in the failure classifier.
- **A blockhash refreshed for `refresh_blockhash` must be re-validated**: a non-refresh retry after the
  ~60s landing wait was resubmitting on an expired blockhash (Jito 400) until we re-checked validity.

## 21. Implementation status & live validation
The **full stack** is wired end-to-end and **proven on mainnet**: the read-only spine runs against the
live SolInfra stream (gRPC `fra.grpc.solinfra.dev:443`), and the **funded submit/landing proof run is
committed** — `logs/lifecycle-log.{json,md}`: 12 bundles landed + 2 AI-recovered injected failures
(`fee_too_low`, `expired_blockhash`) of 14 submissions, every landed bundle
`submitted→processed→confirmed→finalized` with explorer-verifiable slots. Figures below are observed on
the read-only engine and in that committed run:

- **Ingestion → health → sinks.** `prometheon-core::engine` streams Yellowstone slots into the
  `NetworkHealthModel` and fans every `TelemetryEvent` through one `emit`: NATS pub/sub, a
  Postgres/TimescaleDB hypertable (`telemetry_event` + `v_decision`/`v_bundle`/`v_lifecycle`/
  `v_failure` projection views), and a Prometheus `/metrics` exporter. Observed on the read-only
  engine (no wallet needed): slots streaming, congestion reacting to a real leader skip
  (stability `~1.0 → 0.889`), events on the bus, rows in Postgres, `prometheon_*` gauges served.
- **AI in the loop — autonomous retry with fault injection (`prometheon-core::saga`).** On a
  non-landing the core `classify`s the failure from the stream and requests a retry decision over NATS
  (`decision.request.retry`); the agent reasons and returns the levers the engine acts on — the new
  **tip** (`after.tip`) and whether to **refresh** (`after.refresh_blockhash`), enforced by a causal
  contract (a reply omitting them is rejected, not silently treated as a decision). **Division of
  authority (stated plainly):** the agent owns the **autonomous-retry decision** — which lever to pull
  on a failure (refresh the blockhash vs. raise the tip) — and **proposes** the per-bundle tip; the
  deterministic core owns the **safety envelope** — retry-vs-abandon, the attempt cap, an always-forced
  refresh on a real expiry (the model may add a refresh but never remove one), and the competitive
  `[200_000, 1_000_000]` tip clamp before signing. In the committed run most AI tip proposals landed
  below the 200k competitive floor and were lifted to it, so the **floor — not the model's exact number
  — set the tip**; the AI's provable, outcome-changing lever is therefore the retry decision itself (the
  `refresh_blockhash` binary), not the precise tip economics. The core resubmits the next attempt,
  which lands. The agent also **proposes** the per-bundle **tip** (`decision.request.tip`) and makes a
  **submission-timing** call from the live leader schedule that gates the run with a bounded hold.
  Every decision is emitted as `Decision` telemetry → dashboard timeline + the exported log. The whole
  loop, including recovery (attempt 1 failed → attempt 2 landed), is regression-tested with no network
  in `tests/saga_pipeline.rs`. Two small traits (`DecisionSource`, `Submitter`) keep it testable; the
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

The funded mainnet proof run (`scripts/run-proof.sh`) is **complete and committed**:
`logs/lifecycle-log.{json,md}` — 12 landed + 2 AI-recovered injected failures of 14 submissions, with
the explorer-verifiable AI Recovery Chains, 15 real `openai`/Groq decisions, and §19/§20 actuals above.
Remaining before submission is non-code: publish this doc to a public URL and record the demo video.

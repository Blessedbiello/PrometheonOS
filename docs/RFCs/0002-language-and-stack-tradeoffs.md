# RFC 0002 — Language & stack tradeoffs

**Status:** accepted · **Phase:** 0 (foundational) · **Supersedes:** none

This RFC records the *reasoning* behind the stack, as a design review — not a shopping list. The
governing constraint: the system must read as infrastructure built by people who understand
Solana's latency profile, while shipping inside a ~33-day window with a 2-person-equivalent team.

## D1 — Core engine language: **Rust** (vs TypeScript, vs Go)

**Decision: Rust for the hot path.**

| Option | For | Against |
|---|---|---|
| **Rust** ✅ | Predictable latency (no GC pauses in the leader-window catch loop); first-class `yellowstone-grpc-client` with built-in reconnect; `jito-sdk-rust`; strong typed state machines; the credibility signal judges associate with validator/Jito infra | Slower to write; async ergonomics; team ramp |
| TypeScript | Fastest to build; both Jito + Yellowstone have official TS clients | GC jitter is real at the sub-slot timescale we care about; "TS bot" reads as hackathon, not infra |
| Go | Good concurrency, GC tunable | Weaker Yellowstone/Jito SDK story; least differentiation |

The leader-window catch and lifecycle hot path are **latency-sensitive and must be deterministic**;
GC pauses there are exactly the kind of operational blind spot the bounty rewards us for avoiding.
Rust also lets the type system encode the lifecycle/retry state machines so illegal transitions are
unrepresentable — an "engineering maturity" signal.

## D2 — AI layer language: **TypeScript** (vs Python, vs in-Rust)

**Decision: TypeScript, as a separate process.**

The AI is an *asynchronous strategist*, not in the hot path (see RFC 0001 / ARCHITECTURE §3), so its
runtime latency profile is irrelevant — which frees us to optimize for *iteration speed and SDK
quality*. TS gives first-class Anthropic + OpenAI SDKs, trivial JSON/zod structured-output
validation, and shares the generated `contracts/` types with the dashboard. Python was close
(equally good SDKs) but would add a third language and toolchain for no benefit since the dashboard
is already TS/Next.js. Embedding the LLM call in Rust was rejected: worse SDK ergonomics, and it
would blur the **clean AI/core separation** the bounty explicitly grades.

## D3 — Provider abstraction: **pluggable `LlmProvider`** (vs single vendor)

**Decision: abstraction with Anthropic (default) / OpenAI / Ollama adapters, selected by
`LLM_PROVIDER`.** All adapters return one zod-validated `DecisionResult`, so reasoning traces and
dashboard rendering are provider-independent. Rationale: de-risks vendor outage/key issues for the
proof run, demonstrates the reasoning is *in our prompts + schema*, not a single model's magic, and
lets us show a local-model ("no external dependency") story if asked. Cost: one extra interface +
adapters + a `MockProvider` for tests (which we want anyway).

## D4 — Realtime bus: **NATS** (vs Redis Streams, vs in-proc only)

**Decision: NATS.** It provides native **request/reply** (for `decision.request.*`) *and* pub/sub
(for `telemetry.*`) in one tiny single-binary broker — a clean process boundary between the Rust
core and the TS agent/dashboard. Redis Streams does pub/sub well but request/reply is awkward
(we'd hand-roll correlation). An in-process channel was rejected because it would couple the AI
into the core process and undercut the AI/core separation. JetStream is enabled for optional
durable replay of telemetry.

## D5 — Persistence: **Postgres + TimescaleDB** (vs ClickHouse, vs SQLite)

**Decision: Postgres + TimescaleDB.** Our data is *both* relational (bundles, decisions, lifecycle
rows with foreign keys) *and* time-series (slot/health/metric streams). Timescale hypertables give
us time-series performance without giving up relational joins and a single familiar engine.
ClickHouse is superb for pure high-volume analytics but is overkill at our event volume and weaker
for the relational/transactional decision+lifecycle records that are the heart of the lifecycle log.
SQLite was rejected for lack of concurrent writers + time-series ergonomics.

## D6 — Transport to dashboard: **NATS → WebSocket bridge** (vs SSE, vs polling)

**Decision: stream from NATS to the browser over WebSocket** (Next.js route / lightweight bridge),
with Postgres reads for historical/backfill panels. Live telemetry is push-shaped, so WebSocket fits;
SSE was a viable simpler fallback (one-way is all we need) and remains the backup if the WS bridge
costs too much time in Phase 7. Polling rejected — it would contradict the entire "streaming beats
polling" thesis of the project.

## D7 — Cross-language contract: **Rust `schemars` → JSON Schema → TS types** (vs hand-written)

**Decision: generate.** Rust types in `prometheon-types` are the single source of truth; JSON Schema
is emitted to `contracts/`; TS types are generated and runtime-validated with zod; CI fails on drift.
Hand-maintaining parallel type definitions across Rust/TS is a classic source of silent integration
bugs; generation makes the contract *provably* consistent — a "typed telemetry schemas" point the
code-quality criterion rewards.

## Consequences
- Three languages total (Rust, TS, SQL) but only **two app runtimes** (Rust core, Node agent+UI).
- Two infra dependencies beyond the engine (NATS, Postgres/Timescale) + Prometheus for metrics.
- The schema-gen step must exist before cross-process messages are exchanged (wired in Phase 4).
- Live, latency-sensitive behaviours in D1/D4/D6 carry **V (verify)** flags — confirmed against
  SolInfra infra during Phase 1–2 before we trust them in production paths.

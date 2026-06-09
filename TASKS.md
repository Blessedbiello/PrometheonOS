# PrometheonOS — Task List

Single source of truth for work status. Mirrors the in-session task tracker.
Convention: `[ ]` pending · `[~]` in progress · `[x]` done. Every feature is **test-first** (TDD).

> Plan: approved architecture in the project plan. Deadline **2026-06-29**.

---

## Phase 0 — Foundations & scaffolding `[~]`
- [x] git init, `.gitignore`, `.env.example`
- [x] Cargo workspace + 10 crate skeletons (compiles)
- [x] `TASKS.md`, `README.md`
- [x] docs skeletons (`ARCHITECTURE.md`, `FAILURE-TAXONOMY.md`, `TELEMETRY-SCHEMA.md`, `EXPERIMENTS.md`, RFC 0001)
- [x] `infra/docker-compose.yml` (NATS, Postgres+Timescale, Prometheus) + `prometheus.yml`
- [x] CI workflow (rust fmt/clippy/test)
- [~] TS workspace skeleton (`pnpm-workspace.yaml`, `ai-agent` package) — Next.js app deferred to Phase 7
- [ ] `contracts/` schema pipeline (Rust schemars → JSON Schema → TS types) — wired in Phase 4
- [x] Verify: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo build`, `cargo test` green
- [x] dev wallets generated (`wallets/payer.{testnet,mainnet}.json`, gitignored); `.env` written
- [x] `preflight` binary: RPC health + wallet balance + Jito tip-floor + Yellowstone stream check (live tip-floor confirmed working)
- [ ] USER: fund testnet wallet via faucet.solana.com (airdrop IP-rate-limited here)
- [x] USER: claimed SolInfra credits — `Ace` plan (FRA, mainnet); `YELLOWSTONE_ENDPOINT`/`X_TOKEN` + `RPC_URL_MAINNET` in `.env`; validated live (preflight ✓ + engine ✓)
- [ ] USER: fund mainnet wallet ~$20-50 SOL for Phase 8 proof

## Phase 1 — Yellowstone ingestion `[~]`
- [x] shared slot types in `prometheon-types` (`SlotStatus`, `Commitment`, `SlotUpdate`; serde+schemars)
- [x] (test) slot-progression tracking + skipped-slot vs gap distinction — `SlotTracker`, 11 tests green
- [x] (test) gap detection via `parent` chain; reconnect checkpoint + `from_slot` replay plan
- [x] (test) backpressure: bounded queue + explicit drop policy + drop accounting — `BoundedIngestQueue`, 8 tests green
- [x] (test) proto-discriminant status mapping + `SubscribeRequest` builder — `status_map`, `yellowstone`, tests green
- [x] impl live Yellowstone client: slots+tx subscriptions, keepalive ping (both directions), `x-token` auth, TLS — `yellowstone::spawn` (yellowstone-grpc-client 13.1)
- [x] impl reconnect supervise loop (`from_slot` optimistic replay + fallback) + DropNewest backpressure forwarding w/ counters
- [x] (live) Yellowstone smoke test — SolInfra mainnet stream validated end-to-end (preflight + engine: slots → netmodel → NATS)
- [ ] verify provider: inter-slot statuses + `from_slot` buffer size — needs live endpoint
- [ ] later: dedupe replayed updates (lifecycle layer); leader-schedule monitor (Jito `getNextScheduledLeader`, Phase 2)

## Phase 2 — Jito bundle engine `[~]`
- [x] (test) tip-floor parsing (SOL→lamports) + dynamic tip calc (percentile + congestion + bounds, NO hardcoding) — `tip_floor`, `tip`, 6 tests
- [x] (test) priority-fee math (CU price × limit, ceil; +base fee) — `fees`, 5 tests
- [x] (test) bundle assembly: compute-budget ixs first, co-located tip transfer, tip acct is a static key (not ALT), base64 round-trip — `assembly`, 4 tests (solana-sdk 4)
- [x] (test) status parsing (`getInflightBundleStatuses` + `getBundleStatuses`) + domain mapping — `status`, 5 tests
- [x] (test) `getTipAccounts` parsing + rotating tip-account selection — `tip_accounts`, 4 tests
- [x] (test) JSON-RPC seam: envelope/params builders, `result`/`error` unwrap, retryable-status decision — `jsonrpc`, 7 tests
- [x] impl Block Engine HTTP client (reqwest/rustls): `getTipAccounts`, `sendBundle`, inflight + bundle statuses, tip-floor GET; ~1rps pacing + region failover + `x-jito-auth` — `client::BlockEngineClient`
- [ ] `getTipAccounts` cache wrapper (refresh interval) — wire when integrating in core
- [ ] (live, gated) testnet bundle submission lands — needs funded wallet + Jito endpoint

## Phase 3 — Lifecycle + failure `[~]`
- [x] (test) lifecycle state machine: strict transitions, slot/ts capture, latency deltas incl. processed→confirmed (README Q1) — `TransactionLifecycle`, 7 tests
- [x] (test) failure classification w/ confidence + observable-vs-inferred grade + retryability + precedence — `classify`, 11 tests
- [ ] wire lifecycle to live ingest `IngestMessage` (slot/tx → advance) — integration in core
- [ ] wire classifier inputs from `BundleStatuses`/RPC/ingest — integration in core
- [ ] (later) RPC blockhash/height cross-check helper for expiry detection

## Phase 4 — Network model + telemetry `[~]`
- [x] (test) `RollingWindow` (mean/variance/bounded) — 6 tests
- [x] (test) metric fns: slot stability, congestion blend, expiry risk, landing prob, retry success, tip efficiency, cost/landing — 6 tests
- [x] (test) `NetworkHealthModel` composes events → `HealthSnapshot` — 3 tests
- [x] (test) telemetry envelope `TelemetryEvent` + NATS subject mapping + `Decision` contract (serde round-trip) — 5 tests; `FailureClassification` made serde
- [ ] impl telemetry sinks: NATS publisher (async-nats) + Postgres/Timescale sink + Prometheus exporter — needs running services; wire at core integration
- [ ] wire `contracts/` schema-gen (schemars → JSON Schema → TS types) + drift check

## Phase 5 — AI agent `[~]`
- [x] (test) zod schemas mirroring Rust `Decision` contract (`llmDecisionSchema`, `decisionSchema`) — 6 tests
- [x] (test) `MockProvider` deterministic + context-aware (congestion→tip) — 3 tests
- [x] (test) shared prompt builder (tip/timing/retry guidance, JSON-only) — 2 tests
- [x] (test) `decide()` orchestrator composes valid `Decision` (provider/latency/ts/inputs) — 2 tests
- [x] (test) model-output parsing (fences/prose/invalid) — 5 tests
- [x] (test) `providerFromEnv` selection + missing-key errors — 7 tests
- [x] (test) `handleDecisionRequest` parse→decide→serialize — 2 tests
- [x] impl Anthropic/OpenAI/Ollama adapters (selected by `LLM_PROVIDER`); NATS request/reply loop; entrypoint
- [x] CI runs ai-agent typecheck + vitest
- [ ] impl decision-trace persistence (Postgres) — at core integration
- [ ] (live, gated) one real-provider call test (needs an API key)

## Phase 6 — Retry + fault injection `[~]`
- [x] (test) backoff math (exponential, capped, jitter) — 2 tests
- [x] (test) retry policy: per-failure-class refresh/recalc + attempt cap + abandon — 4 tests
- [x] (test) orchestrator: per-saga attempt tracking + backoff scheduling — 1 test
- [x] (test) fault scenarios: blockhash expiry [mandatory], low-tip, delayed, dropped events, congestion — 6 tests
- [x] (test) chaos loop: inject → classify → decide_retry (expiry + low-tip recovery proven) — 2 tests
- [x] `docs/EXPERIMENTS.md` deterministic-loop results documented
- [ ] wire retry orchestrator to the AI agent's reasoned decision (core integration)
- [ ] (live, gated) inject expiry on testnet → agent reasons → refresh+recalc+resubmit lands

## Phase 7 — Dashboard `[~]`
- [x] Next.js 16 + Tailwind v4 dark ops-console scaffold (hand-rolled, no create-next-app)
- [x] (test) telemetry types + pure formatters (lamports/ms/%/slot/truncId/stage/confidence) — 7 tests
- [x] (test) deterministic mock state generator (slots, bundle lifecycles, AI decisions, health) — 4 tests
- [x] panels: Header, NetworkHealth (gauges + p→c delta), SlotStream (Jito✓ flag, next-leader), Bundles (4-stage progression bar + retry/injected), Decisions (timeline + reasoning + confidence bars + before/after)
- [x] mock `/api/telemetry` route; page polls every 1s; production build clean (Next 16, React 19)
- [x] CI typecheck + vitest for the dashboard
- [ ] swap mock for live NATS→SSE/WS bridge at core integration

## Core integration — live wiring + telemetry sinks `[~]`
- [x] config loader (`prometheon-core::config`): network active-set selection + defaults — 6 tests
- [x] NATS telemetry bus (`prometheon-telemetry::nats`): publish events + AI decision request/reply — 5 unit + 1 live round-trip
- [x] read-only engine pipeline (`prometheon-core::engine`): Yellowstone → slot tracker → netmodel → NATS telemetry; **validated live on SolInfra mainnet** (slots streaming, congestion reacting to real skips) — 3 tests
- [x] autonomous saga core (`prometheon-core::submission`): classify→retry decision + AI tip context/extraction/fallback; wallet loader; RPC blockhash/height/validity client — 14 tests. **AI-in-the-loop proven live vs. the agent**: tip re-priced 10.5k→12.5k→14.5k as congestion rose, retries refresh blockhash on expiry then land (`tests/saga_agent.rs`)
- [ ] live on-chain submit + stream-confirmed lifecycle correlation driver — assembled + exercised during the Phase 8 mainnet proof (needs funded wallet)
- [ ] Jito leader-window detection (`getNextScheduledLeader`) feeding submission timing
- [ ] Postgres/Timescale sink + Prometheus `/metrics` exporter (remap docker pg to 5433; 5432 in use)
- [ ] `contracts/` schema-gen (schemars → JSON Schema → TS types) + CI drift check
- [ ] dashboard live bridge (NATS → SSE/WS, replace mock)

## Phase 8 — Mainnet proof + deliverables `[ ]`
- [ ] `scripts/run-proof.sh`: ≥10 mainnet bundles incl ≥2 failures
- [ ] export lifecycle log (JSON + markdown) with explorer-verifiable slots
- [ ] finalize `README.md` (3 answers grounded in real telemetry)
- [ ] publish architecture doc (Notion/Google Docs) — separate public URL
- [ ] record demo video

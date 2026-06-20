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
- [x] swap mock for live NATS bridge — `lib/live.ts` reducer + live route (mock auto-fallback)

## Core integration — live wiring + telemetry sinks `[~]`
- [x] config loader (`prometheon-core::config`): network active-set selection + defaults — 6 tests
- [x] NATS telemetry bus (`prometheon-telemetry::nats`): publish events + AI decision request/reply — 5 unit + 1 live round-trip
- [x] read-only engine pipeline (`prometheon-core::engine`): Yellowstone → slot tracker → netmodel → NATS telemetry; **validated live on SolInfra mainnet** (slots streaming, congestion reacting to real skips) — 3 tests
- [x] autonomous saga core (`prometheon-core::submission`): classify→retry decision + AI tip context/extraction/fallback; wallet loader; RPC blockhash/height/validity client — 14 tests. **AI-in-the-loop proven live vs. the agent**: tip re-priced 10.5k→12.5k→14.5k as congestion rose, retries refresh blockhash on expiry then land (`tests/saga_agent.rs`)
- [ ] live on-chain submit + stream-confirmed lifecycle correlation driver — assembled + exercised during the Phase 8 mainnet proof (needs funded wallet)
- [x] leader-window detection: `NextLeader` parser (bundle, tolerant camel/snake) + `LeaderWindow` math (core: slots-until / in-window / go-hold) + `getNextScheduledLeader` client; wired into the proof readout best-effort — 6 tests. NOTE: exact Jito HTTP endpoint returns 404 on the FRA deployment (searcher `getNextScheduledLeader` may be gRPC-only) → flagged verify-live; degrades gracefully to stream-based timing.
- [x] Postgres/Timescale sink (`prometheon-telemetry::postgres`): `telemetry_event` hypertable + `v_decision`/`v_bundle`/`v_lifecycle`/`v_failure` jsonb views; **validated live** (docker pg on :55432). Prometheus `/metrics` exporter (`prometheon-core::metrics`): live gauges + counters; **validated live** (`prometheon_*` served on :9100). Both wired into the engine fan-out — 4 unit + 1 live pg test
- [x] `contracts/` schema-gen: `schema-gen` bin (schemars → `contracts/json-schema/*`) + `scripts/gen-contracts.sh` (→ `contracts/ts/*.d.ts` via json-schema-to-typescript); JsonSchema derived across the contract types; **CI drift check** (`schema-gen --check`) — 2 tests
- [x] dashboard live bridge: NATS→`DashboardSnapshot` reducer (`dashboard/lib/live.ts`) + live `/api/telemetry` route with mock auto-fallback — 6 tests; **validated live** (real SolInfra mainnet slots streaming + live Jito tip floor in congestion)

## Phase 8 — Mainnet proof + deliverables `[~]`
- [x] live submit driver (`prometheon-core::proof`): assemble→sign→simulate (dry) / `sendBundle` (live) → stream-confirmed lifecycle correlation (`PendingBundles`); `proof` bin. **Dry-run validated live on mainnet** — dynamic tip from live floor (3329 lamports @ congestion 0.131), rotating tip accounts, fresh blockhash + real sig; only funding gates broadcast. 8 tests.
- [x] `scripts/run-proof.sh`: one shared stream + live proof loop (incl. injected failures) + export (built; awaits funded wallet to run)
- [x] lifecycle-log export (`prometheon-telemetry::export` + `export-log` bin): Postgres `telemetry_event` → JSON + explorer-linked markdown (slots, commitment progression, submit→confirmed latency, tip, failure class). **DB→export validated live** on docker pg with synthetic rows — 3 tests. Real data fills it during the proof run.
- [ ] finalize `README.md` (3 answers grounded in real telemetry)
- [ ] publish architecture doc (Notion/Google Docs) — separate public URL
- [ ] record demo video

## Review remediation (2026-06-12) `[x]` — closes the pipeline-integrity gaps a deep review surfaced
- [x] **T1** submit→telemetry→export wired: shared `prometheon-core::sinks` (`EventSink`/`Sinks`), new
  `prometheon-core::proof_run` emits `Bundle`/`Lifecycle`/`Failure` events; network-free regression
  `tests/proof_pipeline.rs` asserts a populated log (≥10 landed + ≥2 classified failures). Was: the
  proof tracked landings in memory only → exported log came out empty.
- [x] **T2** fault injection in the live proof (`--inject low-tip,stale-blockhash`) → guarantees the
  bounty's ≥2 classified failure cases.
- [x] **T3** single shared Yellowstone stream for the whole run; `run-proof.sh` no longer co-launches
  the engine (respects the 1-stream SolInfra plan).
- [x] **T4** AI-chosen tip clamped to policy bounds (`proof::bounded_tip`) — defense-in-depth vs decision poisoning.
- [x] **T5** "landed" now requires stream-`confirmed` (`TransactionLifecycle::reached_confirmed`), not `processed` alone.
- [x] **T6** lifecycle tolerates a missed intermediate stage (forward-skip transitions); backward transitions still rejected.
- [x] **T7** Anthropic default model → `claude-opus-4-8`.
- [x] **T8** dashboard honesty: `DashboardSnapshot.source` (`live|mock`) → badge reads "live"/"simulated", never "live" over mock.
- [x] **T9** CI: advisory `cargo audit` + `pnpm audit` job; `proof_pipeline.rs` runs in `cargo test --workspace`.
- [x] **T10** README/ARCHITECTURE corrected to true scope (read-only spine validated live; submit pipeline integration-tested; live paid proof = gated final step).
- [x] **T11** `EXPERIMENTS.md` filled with the proven deterministic chaos-loop + pipeline results.
- [ ] **T12 (gated on funding)** funded `NETWORK=mainnet ./scripts/run-proof.sh 12` with `LLM_PROVIDER=anthropic` → populated `logs/lifecycle-log.{json,md}` (≥10 bundles, ≥2 failures, explorer-verifiable slots) + a real AI reasoning trace; fill README/EXPERIMENTS live numbers; record demo.

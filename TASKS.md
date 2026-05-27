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
- [ ] Provision: claim SolInfra credits; fund testnet + small mainnet wallet (USER) — blocks live tests

## Phase 1 — Yellowstone ingestion `[~]`
- [x] shared slot types in `prometheon-types` (`SlotStatus`, `Commitment`, `SlotUpdate`; serde+schemars)
- [x] (test) slot-progression tracking + skipped-slot vs gap distinction — `SlotTracker`, 11 tests green
- [x] (test) gap detection via `parent` chain; reconnect checkpoint + `from_slot` replay plan
- [x] (test) backpressure: bounded queue + explicit drop policy + drop accounting — `BoundedIngestQueue`, 8 tests green
- [x] (test) proto-discriminant status mapping + `SubscribeRequest` builder — `status_map`, `yellowstone`, tests green
- [x] impl live Yellowstone client: slots+tx subscriptions, keepalive ping (both directions), `x-token` auth, TLS — `yellowstone::spawn` (yellowstone-grpc-client 13.1)
- [x] impl reconnect supervise loop (`from_slot` optimistic replay + fallback) + DropNewest backpressure forwarding w/ counters
- [ ] (live, gated) testnet smoke test — needs SolInfra endpoint + x-token
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

## Phase 3 — Lifecycle + failure `[ ]`
- [ ] (test) lifecycle state machine transitions + latency deltas
- [ ] (test) failure classification from fixtures (expired blockhash, fee-too-low, compute-exceeded, bundle-failure, +extended)
- [ ] impl stream-driven lifecycle (+ RPC cross-check)
- [ ] impl classifier with confidence

## Phase 4 — Network model + telemetry `[ ]`
- [ ] (test) metric computations (congestion, stability, landing prob, expiry risk, tip efficiency, ...)
- [ ] impl `netmodel` rolling windows + `telemetry.health` snapshots
- [ ] impl telemetry → NATS + Postgres/Timescale sink + Prometheus exporter
- [ ] wire `contracts/` schema-gen + TS type generation + drift check

## Phase 5 — AI agent `[ ]`
- [ ] (test) `LlmProvider` MockProvider + `DecisionResult` zod schema validation
- [ ] (test) decision logic (tip/timing/retry) on synthetic health inputs
- [ ] (test) NATS request/reply round-trip
- [ ] impl Anthropic/OpenAI/Ollama adapters (selected by `LLM_PROVIDER`)
- [ ] impl decision-trace persistence

## Phase 6 — Retry + fault injection `[ ]`
- [ ] (test) retry state machine + backoff + attempt caps
- [ ] (test) chaos assertions: blockhash expiry → detect→reason→refresh→recalc tip→resubmit
- [ ] impl retry orchestrator wired to agent
- [ ] impl fault scenarios (blockhash expiry [mandatory], low-tip, delayed, dropped events, congestion)
- [ ] write `docs/EXPERIMENTS.md` results

## Phase 7 — Dashboard `[ ]`
- [ ] Next.js app scaffold + realtime transport (NATS/WebSocket)
- [ ] panels: slots/leaders, bundles+lifecycle, retries, AI decision timeline, congestion/health

## Phase 8 — Mainnet proof + deliverables `[ ]`
- [ ] `scripts/run-proof.sh`: ≥10 mainnet bundles incl ≥2 failures
- [ ] export lifecycle log (JSON + markdown) with explorer-verifiable slots
- [ ] finalize `README.md` (3 answers grounded in real telemetry)
- [ ] publish architecture doc (Notion/Google Docs) — separate public URL
- [ ] record demo video

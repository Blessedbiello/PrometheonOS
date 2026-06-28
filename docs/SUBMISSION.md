# PrometheonOS — Submission

**Autonomous Solana Execution Intelligence Engine** — for the Superteam *Advanced Infrastructure
Challenge: Build a Smart Transaction Stack*.

PrometheonOS observes the Solana network in real time over Yellowstone gRPC, submits transactions as
Jito bundles with dynamically-computed tips, tracks each across every commitment level from the
**stream**, classifies failures, and lets an **AI agent own a real operational decision** —
*Autonomous Retry with Fault Injection* — with its full reasoning persisted and visible.

## Links
- **Code (open source):** https://github.com/Blessedbiello/PrometheonOS
- **Architecture document (public):** _‹paste Notion / Google-Docs URL here›_ — mirror of
  [`docs/ARCHITECTURE.md`](ARCHITECTURE.md) + [`docs/DIAGRAMS.md`](DIAGRAMS.md).
- **Lifecycle log:** [`logs/lifecycle-log.md`](../logs/lifecycle-log.md) (+ `.json`) — a committed
  mainnet run: **12 bundles landed, 2 AI-recovered failures of 14 submissions**, each landed bundle
  `submitted→processed→confirmed→finalized`; slot numbers verifiable on the explorer (e.g. block
  [429547828](https://explorer.solana.com/block/429547828)).
- **Demo video:** _‹paste link›_ — shot list in [`docs/DEMO-SCRIPT.md`](DEMO-SCRIPT.md).

## Requirement → where it's satisfied

| Bounty requirement | Implementation |
|---|---|
| Architecture doc (public, separate) | `docs/ARCHITECTURE.md` → public URL above; diagrams in `DIAGRAMS.md`. |
| Slot/leader monitoring via Yellowstone gRPC | `prometheon-ingest::yellowstone` — supervised `Subscribe`, reconnect w/ `from_slot` replay, drop-newest backpressure, keepalive/ping. |
| Detect the correct leader window | `rpc::get_slot_leaders` (`getSlotLeaders`) + `leader::LeaderSchedule` (slots-until-rotation) → a live submission-timing decision. (Full Jito-leader classification needs the searcher gRPC + auth; documented.) |
| Construct + submit Jito bundles | `prometheon-bundle` — co-located tip (no tip on a failed bundle), legacy tx (tip account never in an ALT), base64 `sendBundle`, region failover, ~1 rps pacing. |
| Dynamic tips, no hardcoded values | tip from live `tip_floor` percentiles + congestion (`bundle::compute_tip`); the **AI** sets it per bundle, reasoning over the live P50/P75/P95 distribution and targeting the competitive P75–P95 band (P50 sits at the Jito noise floor and rarely lands); the core clamps to a policy band with a competitive floor (`proof::apply_tip_policy`). |
| Lifecycle Submitted→Processed→Confirmed→Finalized (+ ts/slots/deltas) | `prometheon-lifecycle` state machine; deltas + `processed→confirmed` consensus signal captured. |
| Classify expired-blockhash / fee-too-low / compute-exceeded / bundle-failure | `prometheon-failure::classify` — signal-based, confidence + observable-vs-inferred grade (full 18-class taxonomy in `FAILURE-TAXONOMY.md`). |
| Confirm landing via stream subscriptions | `prometheon-core::proof::PendingBundles` correlates our signatures to lifecycles via tx-status + slot-status; RPC is only a cross-check. |
| Retries incl. blockhash refresh on expiry | `prometheon-core::saga` — AI retry decision, core forces a refresh on expiry; `prometheon-retry` is the deterministic fallback. |
| Lifecycle log (≥10, ≥2 failures) | `scripts/run-proof.sh` → `logs/lifecycle-log.{json,md}`; `--inject low-tip:1,stale-blockhash:1` guarantees the failures. |
| AI agent owns one real decision, visible reasoning | **Autonomous Retry with Fault Injection** — see below. |
| README answers (real observations) | `README.md` → "README questions". |
| Reconnection + backpressure | `prometheon-ingest::yellowstone` + `backpressure`. |
| Clean AI/core separation | TS agent ⇄ NATS ⇄ Rust core; the LLM is an async strategist, never in the sub-second hot path. |

## The AI decision it owns — Autonomous Retry with Fault Injection

We deliberately inject a **blockhash expiry** (and a sub-floor tip). On a non-landing the deterministic
core **classifies** the failure and asks the agent, over NATS, *whether and how to recover*. The agent
reasons (refresh the blockhash? re-price the tip, to what?); the core enforces safety invariants (an
expiry **always** forces a refresh; the tip is clamped); it **resubmits** the next attempt — which
lands. The agent also sets the **tip** per bundle and makes a **submission-timing** call from the live
leader schedule. Every decision persists `{inputs, reasoning, confidence, action, before/after,
provider, latency, ts}`, renders on the live dashboard timeline, and is exported into the lifecycle log
("AI Decision Timeline"). This is a reasoned decision in the loop — not sequential automation; the
deterministic policy is only the fallback. Pluggable provider (Anthropic default / OpenAI / Ollama).

The whole AI-driven loop **including recovery** (attempt 1 failed → attempt 2 landed) is regression-
tested with no network in `crates/prometheon-core/tests/saga_pipeline.rs`.

## Run it

```bash
cp .env.example .env                                   # fill RPC/Yellowstone/Jito/wallet/LLM keys
docker compose -f infra/docker-compose.yml up -d       # NATS, Postgres+Timescale, Prometheus
cargo test --workspace                                 # ~180 Rust tests, no network
cargo run -p prometheon-core --bin preflight           # connectivity ✓/✗

# Free dry-run (no funds): validates the whole assembly path against live mainnet
NETWORK=mainnet cargo run -p prometheon-core --bin proof -- --count 12

# Live proof (funded wallet + agent running): the explorer-verifiable log + AI reasoning.
# The committed log was produced with Groq (any OpenAI-compatible host works; or anthropic/ollama):
#   LLM_PROVIDER=openai OPENAI_BASE_URL=https://api.groq.com/openai/v1 OPENAI_MODEL=openai/gpt-oss-120b
pnpm --filter @prometheon/ai-agent start                                 # terminal 1
NETWORK=mainnet ./scripts/run-proof.sh 12 low-tip:1,stale-blockhash:1
cargo run -p prometheon-telemetry --bin export-log     # → logs/lifecycle-log.{json,md}
pnpm --filter @prometheon/dashboard dev                # http://localhost:3000 (live timeline)
```

## Why it should win (judging axes)
- **Does It Work?** Real stack on **mainnet** — the committed run landed **12 bundles and AI-recovered
  2 injected failures** (14 submissions), stream-confirmed through `finalized`, in an explorer-verifiable
  log.
- **Depth of Integration.** No hardcoded shortcuts — live tip floor, dynamic CU, correct commitment
  handling, stream-confirmed landing, real leader schedule, co-located no-ALT tip.
- **AI Demonstration.** The agent drives recovery *during the run* — it prices the tip and chooses
  the refresh escalation (the levers the engine acts on, enforced by a causal contract), while the
  deterministic policy owns the safety gate (retry/abandon, forced-refresh-on-expiry, tip clamp) — the full
  reasoning trace persisted, exported, and rendered live — not a sequential wrapper.
- **Explanation.** Architecture doc + a README grounded in real observations + a network-health model,
  failure taxonomy, fault-injection experiments, and a single cross-language typed contract
  (`schemars` → JSON Schema → TS, CI drift-checked).

## Honest scope notes
- The Jito searcher `getNextScheduledLeader` is a gRPC searcher-API method needing approved auth (its
  HTTP form 404s); we time submission against the RPC leader schedule and rely on the Block Engine
  routing to the next Jito leader. Adding the searcher gRPC is an optional enhancement.
- The dashboard labels its feed honestly (`live` vs `simulated`) — it never shows a live indicator
  over the offline mock.

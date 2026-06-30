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
  `submitted→processed→confirmed→finalized`; the two faults classified from real signals as
  `fee_too_low` and `expired_blockhash`; slot numbers verifiable on the explorer (e.g. block
  [429572113](https://explorer.solana.com/block/429572113)); the failure→recover→land chains render at the
  top of the log as explicit, explorer-verifiable **AI Recovery Chains**, and all 15 decisions in the
  timeline are real `openai`/Groq (zero deterministic fallback).
- **Control room (live dashboard):** `pnpm --filter @prometheon/dashboard dev` → http://localhost:3000 —
  the **Recovery Rail**: the 14 committed bundles ride Submitted→Processed→Confirmed→Finalized; the 2
  injected failures detour through the AI operator (two divergent levers — raise tip vs. refresh blockhash)
  and self-heal to explorer-linked finalized slots. Honest `live | simulated | proof-replay` toggle
  (defaults to a deterministic replay of the committed run — real data, no faked liveness); `?t=34500`
  parks on the both-recovered money shot. The dashboard is the operator's console, **not** the product —
  the product is now a **real callable surface** (Rust library + CLI + loopback HTTP), and a pinned strip shows
  the API `submit(SubmitRequest) → Receipt{ Landed{slot, final_stage, attempts} | Failed{reason, last_class, attempts} }`
  (engine-custody: the engine signs/tips/tracks/retries; see [`docs/INTEGRATION.md`](INTEGRATION.md)).
- **Demo video:** _‹paste link›_ — shot list in [`docs/DEMO-SCRIPT.md`](DEMO-SCRIPT.md); record the
  Recovery Rail at `/?t=34500` (the self-heal) then `/` (the live loop).

## Requirement → where it's satisfied

| Bounty requirement | Implementation |
|---|---|
| Architecture doc (public, separate) | `docs/ARCHITECTURE.md` → public URL above; diagrams in `DIAGRAMS.md`. |
| Slot/leader monitoring via Yellowstone gRPC | `prometheon-ingest::yellowstone` — supervised `Subscribe`, reconnect w/ `from_slot` replay, drop-newest backpressure, keepalive/ping. |
| Detect the correct leader window | `rpc::get_slot_leaders` (`getSlotLeaders`) + `leader::LeaderSchedule` (slots-until-rotation) → a live submission-timing decision. (Full Jito-leader classification needs the searcher gRPC + auth; documented.) |
| Construct + submit Jito bundles | `prometheon-bundle` — co-located tip (no tip on a failed bundle), legacy tx (tip account never in an ALT), base64 `sendBundle`, region failover, ~1 rps pacing. |
| Dynamic tips, no hardcoded values | tip from live `tip_floor` percentiles + congestion (`bundle::compute_tip`); the **AI proposes** it per bundle, reasoning over the live P50/P75/P95 distribution; the deterministic core enforces a competitive `[200_000, 1_000_000]` band (`proof::apply_tip_policy`) — **lifting** a sub-floor proposal to the 200k floor, so in the proof run the floor (not the model's exact number) sets the tip when the AI under-prices. Never hardcoded; never below the competitive floor. |
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
core **classifies** the failure from real signals and asks the agent, over NATS, *whether and how to
recover*. The agent's load-bearing decision is **which lever to pull**: it chose `refresh_blockhash:true`
+ keep-tip for the `expired_blockhash` fault, and keep-blockhash + raise-tip for the `fee_too_low` fault —
two failures, two *different, correct* remedies. A causal contract **rejects** any reply omitting
`after.tip`/`after.refresh_blockhash`, so the model's levers drive the action or the action doesn't happen.
The core enforces safety it cannot override (a real expiry **always** forces a refresh; the tip is clamped
to the competitive `[200_000, 1_000_000]` band) and **resubmits** the next attempt — which lands.

**Honest scope of the AI's authority:** the agent *owns the recovery decision* (the `refresh_blockhash`
binary is the provable, outcome-changing lever) and *proposes* the per-bundle tip — but in the committed
run most tip proposals came in below the 200k competitive floor and were **lifted to it**, so the
deterministic floor, not the model's exact number, set the tip. We sell this as the safety envelope
working, not as the AI owning tip economics. Every decision persists `{inputs, reasoning, confidence,
action, before/after, provider, latency, ts}`, renders on the live dashboard, and exports into the
lifecycle log ("AI Decision Timeline"). A reasoned decision in the loop — not sequential automation.
Pluggable provider (Anthropic / any OpenAI-compatible host / Ollama — the committed proof used Groq `gpt-oss-120b`).

The whole AI-driven loop **including recovery** (attempt 1 failed → attempt 2 landed) is regression-
tested with no network in `crates/prometheon-core/tests/saga_pipeline.rs`.

## Run it

```bash
cp .env.example .env                                   # fill RPC/Yellowstone/Jito/wallet/LLM keys
docker compose -f infra/docker-compose.yml up -d       # NATS, Postgres+Timescale, Prometheus
cargo test --workspace                                 # ~187 Rust tests, no network
cargo run -p prometheon-core --bin preflight           # connectivity ✓/✗

# Free dry-run (no funds): validates the whole assembly path against live mainnet
NETWORK=mainnet cargo run -p prometheon-core --bin proof -- --count 12

# The product surface — hand the engine a strategy, get a Receipt back (it signs/tips/tracks/retries):
NETWORK=mainnet cargo run -p prometheon-core --bin submit -- --serve     # loopback POST /submit
curl -s 127.0.0.1:9180/submit -d '{"transfer_lamports":1,"max_attempts":3,"deadline_secs":180}'
# → {"outcome":"landed","slot":429572113,"final_stage":"finalized","attempts":2}   # see docs/INTEGRATION.md

# Live proof (funded wallet + agent running): the explorer-verifiable log + AI reasoning.
# The committed log was produced with Groq (any OpenAI-compatible host works; or anthropic/ollama):
#   LLM_PROVIDER=openai OPENAI_BASE_URL=https://api.groq.com/openai/v1 OPENAI_MODEL=openai/gpt-oss-120b
pnpm --filter @prometheon/ai-agent start                                 # terminal 1
NETWORK=mainnet ./scripts/run-proof.sh 12 low-tip:1,stale-blockhash:1
cargo run -p prometheon-telemetry --bin export-log     # → logs/lifecycle-log.{json,md}
pnpm --filter @prometheon/dashboard dev                # http://localhost:3000 — the Recovery Rail control room
                                                       #   (defaults to proof-replay; /?t=34500 = the self-heal)
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

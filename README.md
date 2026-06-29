# PrometheonOS

**Autonomous Solana Execution Intelligence Engine.**

PrometheonOS observes the Solana network in real time, submits transactions intelligently as
Jito bundles, tracks their lifecycle across every commitment level, classifies failures, and
lets an AI strategist make and **explain** real operational decisions (tip sizing, submission
timing, autonomous retry). It is built to feel like an internal execution engine used by a
professional Solana infrastructure team — not a transaction sender with an LLM bolted on.

> Built for the Superteam Nigeria *Advanced Infrastructure Challenge — Build a Smart Transaction Stack*.

---

## The AI decision it owns — Autonomous Retry with Fault Injection

The agent **drives the recovery** of a failed bundle. We deliberately inject a blockhash-expiry (and
a sub-floor tip); when a bundle doesn't land, the deterministic core **classifies** the failure from
the stream and asks the agent — over NATS — *how to recover*. The agent reasons in plain English and
returns the concrete levers the engine then acts on: the new **tip** (read from `after.tip`, enforced
by the contract) and whether to **refresh the blockhash** (`after.refresh_blockhash`); the core
**resubmits the next attempt**, which lands.

**Honest division of authority:** the agent owns the *economics and the refresh escalation* (tip
sizing per bundle, the retry's re-price, and adding a refresh), and supplies the visible reasoning;
the deterministic policy (`prometheon-retry`) owns the *safety gate* — it decides retry-vs-abandon and
the attempt cap, **always** forces a blockhash refresh on a true expiry (the model can add one but
never remove it), and the tip is clamped to policy bounds before signing. So this is a genuine
reasoned decision in the loop (not sequential automation), with the core as a safety envelope the
model cannot override. The agent also makes a **submission-timing** call from the live leader
schedule. Every decision persists its full `{inputs, reasoning, confidence, action, before/after}`
trace, renders on the live dashboard timeline, and is exported into the lifecycle log. The saga +
recovery are regression-tested end-to-end without a network in
[`crates/prometheon-core/tests/saga_pipeline.rs`](crates/prometheon-core/tests/saga_pipeline.rs); the
agent's causal contract (it must emit `after.tip`/`after.refresh_blockhash` or the reply is rejected,
never silently treated as a decision) is enforced in `ai-agent` and tested there.

## Why this is different

- **AI genuinely in the loop** — the agent makes tip, timing, and autonomous-retry decisions *during*
  the run; the recovered failure shows attempt 1 (classified failure) → attempt 2 (landed) in the log.
- **Network Health Model** — a live network-condition intelligence layer (congestion, slot
  stability, leader reliability, confirmation-latency variance, bundle landing probability,
  expiry risk) that the AI consumes.
- **Stream-confirmed lifecycle** — landing is confirmed from the **Yellowstone gRPC stream**
  (slot status + tx-status), with RPC only as a cross-check.
- **Dynamic tips, no hardcoding** — tips are computed from live Jito tip-floor percentiles +
  current network conditions, then clamped to policy bounds (defense-in-depth vs. a bad decision).
- **Real leader-window detection** — the upcoming leader schedule from RPC `getSlotLeaders` drives a
  submission-timing decision (the Jito searcher `getNextScheduledLeader` is a gRPC searcher method
  needing approved auth; we time against the RPC schedule and let the Block Engine route to the next
  Jito leader).
- **Visible AI reasoning** — every decision persists `{inputs, reasoning, confidence, action,
  before/after}`, renders on a live decision timeline, and is included in the exported log.
- **Deliberate chaos** — fault injection (blockhash expiry, low tip, …) exercises the AI's
  adaptation; the recovery is captured in the lifecycle log + decision timeline.

## Architecture (high level)

Rust core engine (ingest · bundle · lifecycle · failure · retry · netmodel · telemetry ·
faultinject) ⇄ **NATS** ⇄ TypeScript AI agent (pluggable Anthropic / OpenAI / Ollama) and a
Next.js realtime dashboard, with Postgres + TimescaleDB persistence and Prometheus metrics.

**Key design rule:** the LLM is an asynchronous *strategist* (sets policy, reasons about
failures) — it is **never** in the sub-second leader-window hot path, which stays deterministic
in Rust.

> Full architecture document: _(public Notion/Google Docs link — added at submission)_.
> In-repo source: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Repository layout

```
crates/        Rust workspace (engine)
ai-agent/      TypeScript AI agent (pluggable LLM provider)
dashboard/     Next.js realtime UI
contracts/     JSON Schema (generated from Rust) + generated TS types
infra/         docker-compose: NATS, Postgres+Timescale, Prometheus
docs/          ARCHITECTURE, FAILURE-TAXONOMY, TELEMETRY-SCHEMA, EXPERIMENTS, RFCs
scripts/       proof run + lifecycle-log export
logs/          exported lifecycle logs (explorer-verifiable slots)
```

## Setup

> Prerequisites: Rust (stable ≥1.80), Node 20+, pnpm, Docker.

```bash
cp .env.example .env          # fill in RPC / Yellowstone / Jito / wallet / LLM keys
docker compose -f infra/docker-compose.yml up -d   # NATS, Postgres+Timescale, Prometheus
cargo build                   # build the engine
cargo test                    # unit suite (no network)
```

### Infrastructure preflight

A one-command connectivity check validates everything the engine needs and prints a ✓/✗ report:

```bash
cargo run -p prometheon-core --bin preflight
```

It checks Solana RPC health + wallet balance, Jito tip-floor reachability, and (once configured)
a live Yellowstone slot stream. Use it to confirm your environment before running the engine.

## Running it

With `.env` filled and infra up:

```bash
# 1. Engine — streams Yellowstone slots → network-health model → telemetry sinks
#    (NATS pub/sub, Postgres/Timescale, Prometheus /metrics on :9100).
cargo run -p prometheon-core --bin prometheon

# 2. AI agent — pluggable strategist serving decision.request.* over NATS.
#    LLM_PROVIDER=anthropic|openai|ollama|mock  (mock needs no API key).
LLM_PROVIDER=mock pnpm --filter @prometheon/ai-agent start

# 3. Dashboard — live ops console (auto-falls back to a mock feed when the bus is quiet).
pnpm --filter @prometheon/dashboard dev          # http://localhost:3000

# 4. Proof — assemble + simulate (free dry-run) or submit + stream-track (live) N bundles, with
#    deterministic injected failures. The live run persists Bundle/Lifecycle/Failure telemetry.
NETWORK=mainnet cargo run -p prometheon-core --bin proof -- --count 12                  # dry-run
NETWORK=mainnet ./scripts/run-proof.sh 12 low-tip:1,stale-blockhash:1                   # live (funded wallet)

# 5. Lifecycle log — export the persisted bundles to logs/lifecycle-log.{json,md}.
cargo run -p prometheon-telemetry --bin export-log
```

Regenerate the cross-language contract after changing a Rust telemetry type:

```bash
./scripts/gen-contracts.sh      # Rust (schemars) → contracts/json-schema → contracts/ts
```

## Status

**Validated live (read-only spine).** Ingestion → network-health model → NATS / Postgres /
Prometheus sinks → dashboard, against the SolInfra mainnet stream; plus the AI strategist (tip
decision proven end-to-end over NATS).

**Integration-tested (AI-in-the-loop submit pipeline).** The full path — AI tip decision → submit →
stream-confirmed lifecycle → on failure **classify → AI retry decision → refresh + re-price →
resubmit to landing** → `Bundle`/`Lifecycle`/`Failure`/`Decision` telemetry → Postgres →
lifecycle-log export (with an AI Decision Timeline) — is covered end-to-end, **without a network**, by
`prometheon-core/tests/saga_pipeline.rs` (asserts ≥10 landed, ≥2 classified failures the agent
recovers, and a retry decision with visible reasoning) and `proof_pipeline.rs`. The assembly path is
additionally dry-run validated on mainnet (dynamic tip from live floor, rotating tip accounts, fresh
blockhash + signature; only broadcast needs funding). ~180 Rust + 45 TS tests; CI runs fmt · clippy ·
tests · schema-drift · TS typecheck + tests · dependency audit.

**Proven on mainnet — the funded proof run is committed.** `./scripts/run-proof.sh` opened **one**
Yellowstone stream and submitted bundles **including ≥2 deterministically-injected failures**
(`--inject low-tip:1,stale-blockhash:1`), stream-confirmed each lifecycle, persisted the telemetry, and
exported [`logs/lifecycle-log.{json,md}`](logs/lifecycle-log.md). The committed run:
**12 bundles landed, 2 failed of 14 submissions** — every landed bundle advancing
`submitted→processed→confirmed→finalized`, slots **verifiable on the explorer** (e.g.
[429560175](https://explorer.solana.com/block/429560175)), submit→confirmed deltas of **~0.3–1.8 s** for
most landings (a couple of stragglers to ~5 s), and **real AI decisions** (Groq `gpt-oss-120b`; tip +
retry) in the log's AI Decision Timeline. Both injected faults were classified from real signals —
the sub-floor tip as **`fee_too_low`**, the expired blockhash as **`expired_blockhash`** — and each was
**recovered to landing by the AI retry decision** (re-price / refresh + resubmit). It must be mainnet — Jito has no
devnet Block Engine and the SolInfra stream is mainnet; the free dry-run validates the same assembly
path without funds.

## README questions (answered from real telemetry)

**1. What does the delta between `processed_at` and `confirmed_at` tell you about network health?**

It is the time for a block we have *already seen* (`processed` — in a block, no votes yet,
fork-revertible) to gather a ≥⅔ stake-weighted optimistic vote (`confirmed`). That makes it a
direct read on **consensus health**, not just latency: a small, stable delta (typically
sub-second to ~1–2 s) means high voting participation, a single canonical fork, and votes landing
promptly. A *widening* or high-variance delta is an early warning of lagging vote propagation, fork
contention, or congestion — it appears here *before* it shows up as outright failures. That's why
the network-health model tracks `confirm_latency_variance_ms` and folds it into the
`congestion_score` the AI strategist reasons over.

> Provenance: confirmed by the committed funded run — [`logs/lifecycle-log.md`](logs/lifecycle-log.md)
> records real per-bundle submit→confirmed deltas of **~0.3–1.8 s** for most of the 12 mainnet landings
> (small and stable, exactly the healthy regime described above; a couple of stragglers reach ~5 s),
> each advancing `processed → confirmed → finalized` with explorer-verifiable slots.

**2. Why should you never use `finalized` commitment when fetching a blockhash for a time-sensitive transaction?**

A `finalized` blockhash is already ~31–32 slots (~12.8 s) old when you receive it. Blockhash
validity is a fixed **150-block budget measured from block production, not from when you fetch it** —
so starting from a `finalized` blockhash pre-spends ~20–30% of the window (≈118 usable blocks ≈
47–71 s instead of the full ~60–90 s) for **zero benefit**, sharply raising "blockhash not found" /
expiry risk. Fetch at `confirmed` (or `processed` for maximum runway) and reserve `finalized` for
reading settled state. Our RPC client fetches at `confirmed` for exactly this reason, and expiry is
measured by **block height** (skipped slots don't burn the budget) via `getBlockHeight` +
`isBlockhashValid` — see `prometheon-core::rpc`.

**3. What happens to your bundle if the Jito leader skips their slot?**

The bundle does **not** land (a bundle is atomic — all-or-nothing within one block), the tip is
**not** paid (the tip transfer is co-located in the same transaction as the strategy logic, so a
non-landing costs nothing), and the blockhash **stays valid** (a skipped slot doesn't advance block
height, so the 150-block budget is untouched). The correct response is to retry to the *next* Jito
leader. The catch: only a **Jito-Solana** leader honours bundles, so if no Jito leader remains
inside the blockhash window the bundle is effectively dropped and needs a fresh attempt. Our retry
policy encodes precisely this — `leader_miss` / `skipped_slot` are retryable, the tip is recomputed
from current conditions, and the blockhash is refreshed *only* when the window itself has closed
(`prometheon-retry::policy`).

> Provenance: the read-only engine streams real leader skips (slot stability moving off `1.0`, the
> congestion score rising in response), and the committed funded run exercised the retry path for real —
> both injected faults (a sub-floor tip, an expired blockhash) were classified and **recovered to
> landing by the AI retry decision**, recorded in [`logs/lifecycle-log.md`](logs/lifecycle-log.md).

## License

MIT.

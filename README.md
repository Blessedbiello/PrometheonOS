# PrometheonOS

**Autonomous Solana Execution Intelligence Engine.**

PrometheonOS observes the Solana network in real time, submits transactions intelligently as
Jito bundles, tracks their lifecycle across every commitment level, classifies failures, and
lets an AI strategist make and **explain** real operational decisions (tip sizing, submission
timing, autonomous retry). It is built to feel like an internal execution engine used by a
professional Solana infrastructure team — not a transaction sender with an LLM bolted on.

> Built for the Superteam Nigeria *Advanced Infrastructure Challenge — Build a Smart Transaction Stack*.

---

## Why this is different

- **Network Health Model** — a live network-condition intelligence layer (congestion, slot
  stability, leader reliability, confirmation-latency variance, bundle landing probability,
  expiry risk) that the AI consumes.
- **Stream-confirmed lifecycle** — landing is confirmed from the **Yellowstone gRPC stream**
  (slot status + tx-status), with RPC only as a cross-check.
- **Dynamic tips, no hardcoding** — tips are computed from live Jito tip-floor percentiles +
  current network conditions.
- **Visible AI reasoning** — every decision persists `{inputs, reasoning, confidence, action,
  before/after}` and renders on a live decision timeline.
- **Deliberate chaos** — fault injection (blockhash expiry, low tip, delayed submission,
  dropped stream events, congestion) exercises the AI's adaptation; results are documented.

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

# 4. Proof — build + simulate (free dry-run) or submit (live) N bundles on mainnet.
NETWORK=mainnet cargo run -p prometheon-core --bin proof -- --count 12            # dry-run
NETWORK=mainnet ./scripts/run-proof.sh 12                                         # live (needs funded wallet)

# 5. Lifecycle log — export the persisted bundles to logs/lifecycle-log.{json,md}.
cargo run -p prometheon-telemetry --bin export-log
```

Regenerate the cross-language contract after changing a Rust telemetry type:

```bash
./scripts/gen-contracts.sh      # Rust (schemars) → contracts/json-schema → contracts/ts
```

## Status

The full stack is built and **validated live against the SolInfra mainnet stream**: ingestion →
network-health model → NATS / Postgres / Prometheus sinks → dashboard, plus the AI strategist
(tip decision proven end-to-end over NATS) and the bundle submit path (dry-run validated on
mainnet — dynamic tip from live floor, rotating tip accounts, fresh blockhash + signature; only
broadcast needs funding). ~160 tests; CI runs fmt · clippy · tests · schema-drift · TS typecheck +
tests.

The one remaining step is the **mainnet proof run** (≥10 bundles incl. ≥2 failures), which needs a
funded mainnet wallet. Fund `wallets/payer.mainnet.json` with ~$5–15 of SOL, then run
`./scripts/run-proof.sh`. (It must be mainnet — Jito has no devnet Block Engine and the SolInfra
stream is mainnet; the dry-run validates the same path for free in the meantime.)

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

> Live: against the SolInfra mainnet stream we watched slot status advance
> `processed → confirmed → finalized` in real time and saw stability/congestion react to real leader
> skips within a single run. Exact per-bundle submit→confirmed deltas are recorded in
> [`logs/lifecycle-log.md`](logs/) from the proof run.

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

> Live: we observed real leader skips on mainnet — slot stability fell from `1.0` to `0.889` and the
> congestion score rose in response within one run. Captured skipped-slot / leader-miss telemetry is
> in the proof-run log.

## License

MIT.

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

_Run instructions for the engine, AI agent, dashboard, and the mainnet proof script are added
as each phase lands (see [`TASKS.md`](TASKS.md))._

## README questions (answered from real telemetry)

These are answered with observations from our running system at submission time:

1. **What does the delta between `processed_at` and `confirmed_at` tell you about network health?** _(answer + measured deltas added in Phase 8)_
2. **Why should you never use `finalized` commitment when fetching a blockhash for a time-sensitive transaction?** _(answer + measured runway difference added in Phase 8)_
3. **What happens to your bundle if the Jito leader skips their slot?** _(answer + captured skipped-slot event added in Phase 8)_

## License

MIT.

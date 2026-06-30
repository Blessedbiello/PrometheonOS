# Integrating PrometheonOS

PrometheonOS is headless infrastructure. The dashboard is the operator's control room; the **product
is a callable surface**: you hand the engine a strategy to land, and it builds, signs, tips, tracks the
full lifecycle, and **autonomously retries** on a non-landing — returning a `Receipt`.

There are three ways to call it, all backed by the *same* tested saga (`run_saga`): a Rust **library
function**, a **CLI**, and a loopback **HTTP endpoint**.

---

## The contract

```
submit(SubmitRequest) → Receipt
```

```rust
pub struct SubmitRequest {
    pub strategy: SubmitStrategy,   // SelfTransfer { lamports }  (v1; arbitrary ixs = future work)
    pub signer: SignerSource,       // ConfigWallet | KeypairPath(String)
    pub max_attempts: u32,          // attempts before abandoning
    pub deadline: Duration,         // wall-clock budget incl. autonomous retries
}

pub enum Receipt {
    Landed  { slot: u64, final_stage: String, attempts: u32 },   // final_stage: "confirmed" | "finalized"
    Failed  { reason: String, last_class: Option<String>, attempts: u32 },
}
```

A landed receipt reports the **landed attempt's** slot — if the first attempt failed and the AI
recovered it on attempt 2, you get `Landed { attempts: 2, slot: <attempt-2 slot> }`. A `Failed` receipt
carries the real-signal classification of the final attempt (`expired_blockhash`, `fee_too_low`, …).

**The receipt is the lifecycle log.** A `Receipt` is derived from the *same* `Bundle`/`Lifecycle`/
`Failure` telemetry the engine emits, via the same `export::build_log` assembler that produces the
committed `logs/lifecycle-log.json`. So a receipt is always reconcilable with the exported log — it is
not a separate, drift-prone codepath.

### Engine-custody (and why)

v1 is **engine-custody**: the engine holds the wallet and signs. This is deliberate and honest — the
saga **refreshes the blockhash and re-signs** on a retry-after-expiry, which requires the signing key.
A caller-signed `submit(signed_tx)` that still supports autonomous refresh-on-expiry needs a **durable
nonce** (or a re-sign callback); that's documented future work, so the autonomous retry we advertise is
always real. (See `docs/RFCs/0003-retry-orchestrator.md`.)

---

## 1. CLI

```bash
# one-shot: submit a self-transfer strategy, print the Receipt JSON
NETWORK=mainnet cargo run -p prometheon-core --bin submit -- \
  --transfer-lamports 1 --max-attempts 3 --deadline-secs 180
```

```json
{ "outcome": "landed", "slot": 429572113, "final_stage": "finalized", "attempts": 1 }
```

Free, no funding needed: run it against **devnet** (`NETWORK=devnet`) to exercise the whole path end to
end, or use the proof runner's dry-run (`--bin proof` without `--live`) to validate assembly against
live mainnet without broadcasting.

## 2. HTTP (loopback)

```bash
# serve POST /submit on 127.0.0.1:9180 (override with SUBMIT_ADDR)
NETWORK=mainnet cargo run -p prometheon-core --bin submit -- --serve

curl -s 127.0.0.1:9180/submit \
  -d '{"transfer_lamports":1,"max_attempts":3,"deadline_secs":180}'
# → {"outcome":"landed","slot":429572113,"final_stage":"finalized","attempts":2}
```

**Security posture (read this).** The endpoint signs with the engine's funded wallet, so — exactly like
the Prometheus `/metrics` exporter and the dashboard — **localhost is the trust boundary**. It binds
loopback-only and **refuses to bind a non-loopback host**; it is unauthenticated by design and must not
be exposed. The request body carries only `{transfer_lamports, max_attempts, deadline_secs}`; the signer
is loaded **server-side** and never crosses the wire or a log. A token/mTLS auth layer is future work.
A machine-readable spec is in [`contracts/openapi-submit.yaml`](../contracts/openapi-submit.yaml).

## 3. Rust library

```rust
use prometheon_core::{config::Config, submit::{self, SubmitRequest, SubmitStrategy, SignerSource}};
use std::time::Duration;

let config = Config::from_env()?;
let receipt = submit::submit(&config, SubmitRequest {
    strategy: SubmitStrategy::SelfTransfer { lamports: 1 },
    signer: SignerSource::ConfigWallet,
    max_attempts: 3,
    deadline: Duration::from_secs(180),
}).await?;
```

### Deep integration — the saga seam

For full control (custom telemetry sinks, your own decision source, your own submitter), drive the saga
directly. The engine is generic over three traits in `crates/prometheon-core/src/saga.rs`:

- `Submitter` — builds/signs/sends one attempt and returns a `SubmittedBundle` (+ an optional real-error
  `probe_failure`). The product impl is `EngineSubmitter`; the proof impl is `LiveSubmitter`.
- `DecisionSource` — answers the AI tip/retry/timing decisions (live: NATS → the TS agent; falls back to
  the deterministic policy on timeout). The product impl is `NatsDecisionSource`.
- `EventSink` — where telemetry fans out (`Sinks` = NATS + Postgres in production).

`run_submit(sink, decider, submitter, bases, rx, cfg) -> Vec<Receipt>` runs the saga and returns one
receipt per logical bundle. Because the three traits are injectable, the entire AI-driven loop — tip
decision, injected failure, classification, autonomous retry, recovery — is exercised with **no network**
in `crates/prometheon-core/tests/saga_pipeline.rs` and `tests/submit_receipt.rs`. Those tests are the
worked example.

---

## What you need running

- **RPC + Jito Block Engine + Yellowstone gRPC** endpoints (the `*_MAINNET` set when `NETWORK=mainnet`),
  configured in `.env` — see `.env.example`. Yellowstone is required: landing is confirmed from the
  stream, not RPC polling.
- **NATS** (`docker compose -f infra/docker-compose.yml up -d`) for the AI agent's decisions and the
  telemetry bus. If the agent is unreachable, the saga degrades to the deterministic policy and still
  returns a receipt.
- **Postgres/TimescaleDB** (optional) to persist the lifecycle log for `export-log`.
- A **funded wallet** to land on mainnet (devnet SOL is free for testing).

## Headless operation (no dashboard)

The dashboard is never required. Run the engine + agent and integrate via the surface above:

```bash
docker compose -f infra/docker-compose.yml up -d        # NATS · Postgres/Timescale · Prometheus
pnpm --filter @prometheon/ai-agent start                # the AI strategist over NATS
cargo run -p prometheon-core --bin submit -- --serve    # the submit endpoint (or call the library)
```

## Future work (kept honest)

- **Caller-custody `submit(signed_tx)`** with durable-nonce so a caller-signed tx survives a
  refresh-on-expiry retry.
- **Arbitrary strategies** (`SubmitStrategy::Custom { ixs }`) — thread `strategy_ixs` through
  `proof::prepare_attempt` (currently hardcoded to the self-transfer).
- **HTTP auth** (token / mTLS) so the endpoint can move off loopback.

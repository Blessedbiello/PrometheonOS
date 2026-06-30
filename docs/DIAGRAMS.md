# PrometheonOS — Diagrams

Mermaid diagrams (render on GitHub). These are the canonical source for the figures in the public
architecture document — all reflect the implemented system.

---

## 1. System context / data flow

```mermaid
flowchart TB
  subgraph SOL["Solana (testnet / mainnet)"]
    YS["Yellowstone gRPC<br/>(slots · leader · tx)"]
    JBE["Jito Block Engine<br/>(sendBundle · statuses · tip accts)"]
    TF["Jito tip-floor API<br/>(bundles.jito.wtf)"]
    RPC["RPC node<br/>(blockhash · CU · cross-check)"]
  end

  subgraph CORE["Rust core (tokio) — deterministic hot path"]
    ING["ingest"]
    BUN["bundle"]
    LIF["lifecycle"]
    FAIL["failure"]
    RET["retry"]
    NET["netmodel"]
    FI["faultinject"]
    TEL["telemetry"]
  end

  subgraph BUS["NATS"]
    direction LR
    T1["telemetry.*"]
    D1["decision.request.*"]
    D2["decision.*"]
  end

  AGENT["TS AI agent<br/>LlmProvider: Claude / OpenAI / Ollama"]
  DASH["Next.js dashboard"]
  DB[("Postgres + TimescaleDB")]
  PROM["Prometheus"]

  YS --> ING
  TF --> BUN
  JBE <--> BUN
  RPC <--> BUN
  RPC <--> RET

  ING --> LIF --> FAIL --> RET --> BUN
  ING --> NET
  LIF --> NET
  FI -. perturbs .-> BUN
  FI -. perturbs .-> ING

  ING & BUN & LIF & FAIL & RET & NET --> TEL
  TEL --> T1
  RET -- "needs decision" --> D1
  D1 --> AGENT
  AGENT -- Decision --> D2
  D2 --> CORE
  T1 --> AGENT
  T1 --> DASH
  D2 --> DASH
  TEL --> DB
  AGENT --> DB
  DASH --> DB
  TEL --> PROM
```

---

## 2. Transaction lifecycle state machine

Driven primarily by the Yellowstone stream (slot status + tx status); RPC is a cross-check.

```mermaid
stateDiagram-v2
  [*] --> Submitted: sendBundle ok (bundle_id)
  Submitted --> Processed: tx seen in block (stream) / slot PROCESSED
  Processed --> Confirmed: slot CONFIRMED (≥⅔ stake vote)
  Confirmed --> Finalized: slot FINALIZED (rooted, 32-deep)
  Finalized --> [*]: landed (terminal success)

  Submitted --> Failed: on-chain err / bundle Failed
  Processed --> Failed: slot DEAD (fork dropped)
  Confirmed --> Failed: slot DEAD (fork dropped)
  Submitted --> Expired: blockHeight > lastValidBlockHeight
  Submitted --> Dropped: leader skipped + no land in window

  Failed --> [*]: terminal (this attempt)
  Expired --> [*]: terminal (this attempt)
  Dropped --> [*]: terminal (this attempt)

  note right of Dropped
    A non-landing (Failed / Expired / Dropped) is terminal for THIS
    attempt. The saga-level retry orchestrator (§3 / RFC 0003) then
    classifies the failure and may launch a fresh attempt — a NEW
    lifecycle starting again at Submitted. Retrying / Abandoned are
    NOT lifecycle stages.
  end note
```

Each transition records `{slot, ts, delta_ms_from_prev}` → `LifecycleEvent`. Forward-skips
(e.g. `Submitted → Confirmed`, `Processed → Finalized`) are accepted when the stream delivers a
later commitment without the intermediate one; backward/illegal transitions are rejected.

Retry is orchestrated at the saga level (see §3 / RFC 0003), not as a lifecycle stage.

---

## 3. Retry orchestrator — the saga loop (RFC 0003)

A *model* of the real saga loop in `crates/prometheon-core/src/saga.rs` (`run_saga` →
`reconcile` → `fail_and_retry` → `resolve_retry` → `launch`). The "state" is per-base bookkeeping
over the one shared Yellowstone stream, not an enum — see RFC 0003.

```mermaid
stateDiagram-v2
  [*] --> InFlight: launch attempt (Submitted)
  InFlight --> Landed: reached confirmed (drains to finalized)
  Landed --> [*]

  InFlight --> Classify: non-landing (failed tx-status OR give-up watermark)
  Classify --> Decide: FailureClass (probe_failure → real RPC/Jito signals; else heuristic)
  Decide --> Resolve: emit Failure, ask AI for Decision over NATS (None → deterministic policy)
  Resolve --> Abandon: attempt cap OR non-retryable class
  Resolve --> Launch: retry plan (forced refresh-on-expiry; AI tip, clamped)
  Launch --> InFlight: attempt n+1 (new bundle_id, Submitted)
  Abandon --> [*]
```

Every resubmit is justified by a persisted AI `Decision`; the forced refresh-on-expiry and the
attempt cap are deterministic safety the model cannot remove. Modeled as a saga over injectable
traits (RFC 0003), regression-tested with no network in `tests/saga_pipeline.rs`.

---

## 4. AI decision pipeline

```mermaid
sequenceDiagram
  participant Core as Rust core
  participant NATS
  participant Agent as TS AI agent
  participant LLM as LlmProvider
  participant DB as Postgres

  Note over Core: failure / retry / timing trigger (off hot path)
  Core->>NATS: request decision.request.<type> {ctx, health snapshot}
  NATS->>Agent: deliver request
  Agent->>Agent: assemble inputs (tip floor, congestion, history)
  Agent->>LLM: decide(DecisionRequest)  [structured output]
  LLM-->>Agent: {decision, reasoning, confidence, action}
  Agent->>Agent: zod-validate against DecisionResult schema
  Agent-->>NATS: reply Decision (+ publish decision.<type>)
  NATS-->>Core: Decision
  Core->>Core: apply policy (tip target / hold-go / retry plan)
  Agent->>DB: persist reasoning trace
  Note over Core: timeout → fall back to last cached policy (never block hot path)
```

---

## 5. Event flow timeline (single bundle, happy path)

```mermaid
sequenceDiagram
  participant E as Engine
  participant J as Jito BE
  participant Y as Yellowstone
  E->>E: leader window detected (ingest)
  E->>E: tip = f(tip_floor, congestion)  ← agent policy
  E->>J: sendBundle (tip co-located in strategy tx)
  J-->>E: bundle_id  (t0 = Submitted)
  Y-->>E: tx in block / slot PROCESSED  (t1 → processed_latency)
  Y-->>E: slot CONFIRMED                (t2 → confirmed_latency)
  Y-->>E: slot FINALIZED                (t3 → finalized_latency)
  E->>E: emit LifecycleEvents + metrics
```

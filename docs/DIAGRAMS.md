# PrometheonOS — Diagrams

Mermaid diagrams (render on GitHub). These are the canonical source for the figures in the public
architecture document. Marked _(target)_ where they describe behaviour implemented in a later phase.

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

## 2. Transaction lifecycle state machine _(target — Phase 3)_

Driven primarily by the Yellowstone stream (slot status + tx status); RPC is a cross-check.

```mermaid
stateDiagram-v2
  [*] --> Submitted: sendBundle ok (bundle_id)
  Submitted --> Processed: tx seen in block (stream) / slot PROCESSED
  Processed --> Confirmed: slot CONFIRMED (≥⅔ stake vote)
  Confirmed --> Finalized: slot FINALIZED (rooted, 32-deep)
  Finalized --> [*]

  Submitted --> Failed: on-chain err / bundle Failed
  Processed --> Failed: slot DEAD (fork dropped)
  Submitted --> Expired: blockHeight > lastValidBlockHeight
  Submitted --> Dropped: leader skipped + no land in window

  Failed --> Retrying: classifier + AI says retryable
  Expired --> Retrying: refresh blockhash + recalc tip
  Dropped --> Retrying: rebroadcast to next Jito leader
  Retrying --> Submitted: resubmit (attempt n+1)
  Retrying --> Abandoned: attempt cap / non-retryable
  Abandoned --> [*]
```

Each transition records `{slot, ts, delta_ms_from_prev}` → `LifecycleEvent`.

---

## 3. Retry orchestrator state machine _(target — Phase 6; RFC 0003)_

```mermaid
stateDiagram-v2
  [*] --> Idle
  Idle --> Classifying: failure event
  Classifying --> Deciding: class + confidence
  Deciding --> Abandon: non-retryable OR attempts ≥ cap
  Deciding --> Preparing: AI returns retry plan
  Preparing --> RefreshBlockhash: blockhash invalid / expiry-risk high
  Preparing --> RecalcTip: tip below target for current conditions
  RefreshBlockhash --> RecalcTip
  RecalcTip --> Backoff
  Preparing --> Backoff: no param change needed
  Backoff --> Resubmit: jittered delay elapsed
  Resubmit --> Idle: new bundle_id handed to lifecycle
  Abandon --> [*]
```

Every entry into `Resubmit` is justified by a persisted AI `Decision` (no hardcoded retry flow).

---

## 4. AI decision pipeline _(target — Phase 5)_

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

## 5. Event flow timeline (single bundle, happy path) _(target)_

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

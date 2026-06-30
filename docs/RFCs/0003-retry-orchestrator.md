# RFC 0003 — Autonomous retry orchestrator: a saga over injectable traits

**Status:** accepted · **Phase:** 6 (implemented in `prometheon-core::saga`)

## Context
The bounty's headline AI decision is **Autonomous Retry with Fault Injection**: on a non-landing the
system must classify the failure from real signals, ask the AI how to recover, and resubmit — with no
hardcoded retry flow. We needed an orchestrator that (a) makes the AI's retry decision load-bearing,
(b) enforces deterministic safety invariants the model can't violate, and (c) is fully exercisable with
no network so the recovery path is regression-tested, not just demoed.

## Decision
Implement retry as a **saga** (`crates/prometheon-core/src/saga.rs`, `run_saga`) rather than an explicit
state-machine enum. The saga drives every attempt over the one shared Yellowstone stream and, on a
non-landing, runs this loop:

```
attempt fails (failed tx-status, or give-up watermark passed)
  → classify the failure        (probe_failure → real RPC/Jito signals; else heuristic)
  → emit Failure telemetry
  → ask the AI for a retry decision   (DecisionSource over NATS; None → deterministic policy)
  → resolve_retry(class, attempt, max, ai)   ← combine AI choice with safety invariants
       · abandon at the attempt cap / for non-retryable classes
       · a class-forced refresh (expiry/timeout) is NEVER downgraded, even if the model omits it
       · the AI's tip flows through (clamped to the competitive band by the submitter)
  → launch attempt n+1   (refresh blockhash? re-price? per the resolved plan)
  → … until landed (confirmed) or abandoned
```

Live I/O sits behind three injectable traits — `Submitter`, `DecisionSource`, `EventSink` — so the same
`run_saga` runs against fake doubles with scripted streams in `tests/saga_pipeline.rs` and
`tests/submit_receipt.rs`.

## Rationale
A saga (orchestration over typed steps + injected effects) fits better than a hand-rolled state enum
here: the "state" is mostly the per-base bookkeeping (`current_attempt`, `last_tip`, `landed`, `done`)
plus the stream position, and the interesting logic is the **decision composition** in `resolve_retry`,
not state transitions. Keeping effects behind traits is what makes the AI-in-the-loop recovery
deterministically testable — the property that matters most for credibility.

## Consequences
- The retry behavior is **real and tested** (recovery: attempt 1 fails → attempt 2 lands, asserted with
  no network), and it is the engine behind both the proof runner and the `submit → Receipt` product
  surface (RFC: the receipt is derived from the saga's emitted telemetry).
- The classic "retry orchestrator state machine" diagram (`docs/DIAGRAMS.md` §3) is a *model* of this
  saga loop, not a literal enum. An explicit `Retrying`/`Abandoned` lifecycle state machine is a possible
  future refactor; it would not change behavior, only make the diagram enum-backed.
- Safety is deterministic and cannot be removed by a poisoned/garbled AI reply: the attempt cap,
  non-retryable classes, the forced refresh-on-expiry, and the tip clamp all live in the core.

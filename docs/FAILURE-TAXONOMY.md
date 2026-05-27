# Failure Taxonomy

For each failure: **detection signal · telemetry signals · retryability · AI interpretation · remediation.**
Rows marked ★ are explicitly required by the bounty. Detection is implemented test-first in
`prometheon-failure` (Phase 3); every classification carries a **confidence score** so the AI can
reason about ambiguity rather than asserting false certainty.

## Epistemics: what we can *observe* vs what we *infer*

A core honesty principle: we do not pretend to see validator internals we cannot see. Each class is
labelled by **evidence grade**:

- **O — Observable.** Directly visible in our data (on-chain `err`, Yellowstone slot/tx status,
  block height vs `lastValidBlockHeight`, `getBundleStatuses`). High-confidence classification.
- **I — Inferred.** Not directly visible; deduced from a *pattern* of observable signals (e.g. we
  cannot see a leader's TPU queue, so "TPU saturation" is inferred from skipped-slot clustering +
  rising confirm-latency variance + non-landing despite adequate tip). Confidence is bounded and
  the trace states the inference explicitly.
- **V — Verify experimentally.** Behaviour we believe but must confirm against live infra before
  trusting in classification thresholds (flagged in the plan's "unverified facts").

Confidence is a function of evidence grade × signal agreement. An `O` class with a matching
on-chain error scores ~0.95; an `I` class from a 3-signal pattern scores ~0.6–0.8 and the
reasoning trace names the inference.

## Matrix

| # | Class | Grade | Detection signal(s) | Retryable? | AI interpretation | Remediation |
|---|---|---|---|---|---|---|
| 1 | ★ Stale / expired blockhash | O | block height > `lastValidBlockHeight`; `isBlockhashValid`=false; no landing in window | Yes | window exhausted before inclusion | refresh blockhash @ `confirmed`, recalc tip, resubmit |
| 2 | ★ Insufficient prioritization fee ("fee too low") | I | no landing + no on-chain err + eventual expiry; tip/CU-price below current floor; congestion up | Yes | lost the priority/tip auction | raise tip + CU-price toward higher percentile, resubmit |
| 3 | ★ Compute exhaustion ("compute exceeded") | O | on-chain `ComputeBudgetExceeded`; lands + fee charged | No (as-is) | CU limit too low or logic too heavy | raise `SetComputeUnitLimit` / fix logic, then resubmit |
| 4 | ★ Bundle rejection / failure | O | `getBundleStatuses.err`; inflight `Failed` across all regions | Depends | block engine/sim rejected or simulation reverted | inspect cause; resubmit if transient |
| 5 | Leader miss | I | scheduled leader's slot produced no accepted block (slot stream); `parent` gap at that slot | Yes | leader offline / block uncled | rebroadcast to next (Jito) leader window |
| 6 | Skipped slot | O | slot stream shows slot skipped (no `COMPLETED`/`PROCESSED`); `parent` jumps | Yes | normal-ish; bundle didn't land here | retry into next produced slot |
| 7 | Network congestion | I | high tip floor + rising confirm-latency + skipped-slot rate over window → `congestion_score`↑ | Yes | blockspace contention | raise tip, optionally hold for better window |
| 8 | TPU saturation | I | non-landing despite adequate tip + clustered skips + confirm-latency variance spike (cannot see TPU queue directly) | Yes | leader ingestion overloaded / drop at fetch stage | back off, retime to a healthier leader, raise tip |
| 9 | RPC inconsistency | O | stream vs RPC disagree on status/slot; RPC lags stream | n/a | RPC node behind canonical view | trust stream; flag + reconcile; rotate RPC |
| 10 | Stream lag / disconnect | O | reconnect fired; `from_slot` gap; slot-arrival latency↑; missed pings | n/a | our ingestion fell behind / dropped | reconnect + `from_slot` replay + RPC cross-check |
| 11 | Delayed propagation | I | tx seen at `processed` but `processed→confirmed` delta far above baseline (Turbine/vote lag) | Wait | shred/vote propagation slow, not a hard fail | wait within window before deciding; don't over-retry |
| 12 | Account contention | I | repeated write-lock conflicts on hot accounts; non-landing pattern tied to specific accounts | Yes | competing writers on same account locks | retime submission; reduce contention |
| 13 | Duplicate signature | O | sigverify dedup / `AlreadyProcessed`; same sig already in a block | No | identical tx already landed/in-flight | treat prior as canonical; rebuild w/ fresh blockhash if new intent |
| 14 | Invalid simulation state | O | preflight/simulate error before submit | No | tx would revert given current state | fix tx / refresh state, rebuild |
| 15 | Transaction too large | O | serialized length > packet limit at build time | No | too many ixs/accounts | split, or use ALT (never for the tip account) |
| 16 | Instruction error | O | on-chain `InstructionError(index, ..)` | Depends | program-level revert at instruction `index` | decode error, decide fixable vs fatal |
| 17 | Confirmation timeout | O | no status by deadline while blockhash still valid | Yes | inclusion slow but window open | resubmit/rebroadcast while valid; then reclassify as #1 |
| 18 | Tip auction outbid | I | landed slot exists but our bundle absent; competing higher tip/CU-efficiency observed | Yes | another searcher won the auction | raise tip toward 95th pct or improve tip/CU ratio |

## Classification flow (high level)

```
on-chain err present? ──yes─► map err → {compute exceeded | instruction error | dup sig} (grade O)
        │ no
        ▼
landed anywhere? ──no─► within blockhash window?
        │ yes              │ yes ─► timeout/propagation (O/I) → wait/resubmit
        ▼                  │ no  ─► stale blockhash (O, #1)
   reconcile RPC vs stream      │
   (RPC inconsistency #9)       └─ no land + expired + tip<floor ─► fee too low (I, #2)
```

Each `O` decision is asserted from a single authoritative signal; each `I` decision requires ≥2
agreeing signals and the reasoning trace must state the inference and its confidence.

_Each row gets a worked example + a real telemetry excerpt in Phase 8, drawn from the mainnet proof
run and the fault-injection experiments._

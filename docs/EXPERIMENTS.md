# Experiments & Fault-Injection Methodology

We deliberately perturb the pipeline and record how the AI strategist adapts. Each scenario:
**hypothesis · injection · expected telemetry · observed result · AI decision trace.**
Implemented in `prometheon-faultinject` (Phase 6); results filled from real runs (Phase 6/8).

## Scenarios
1. **Blockhash expiry (mandatory).** Submit with a stale blockhash / hold past
   `lastValidBlockHeight`. Expect: classify `expired_blockhash` (high confidence) → agent refreshes
   blockhash @ `confirmed`, recalcs tip, resubmits. _(result pending)_
2. **Low-tip submission.** Tip below floor → expect non-landing → agent escalates tip toward a
   higher percentile. _(result pending)_
3. **Delayed submission.** Inject latency before submit → expect rising `expiry_risk_score` and
   timing holds. _(result pending)_
4. **Dropped stream events.** Force Yellowstone disconnect/gap → expect reconnect + `from_slot`
   replay + intact lifecycle (no false "dropped"). _(result pending)_
5. **Simulated congestion.** Synthetic high `congestion_score` → expect tip/timing adaptation.
   _(result pending)_

## Deterministic recovery loop (proven in tests)

Before the live runs, the chaos → classify → recover loop is proven deterministically in
`prometheon-faultinject/tests/chaos_loop.rs`:

- **Blockhash expiry (mandatory):** `FaultScenario::BlockhashExpiry` → `classify` returns
  `ExpiredBlockhash` (confidence ≥ 0.9, retryable) → `decide_retry` returns
  `Retry { refresh_blockhash: true, recalc_tip: true, next_attempt: 2 }`. The engine refreshes the
  blockhash, re-prices the tip from live data, and resubmits.
- **Low tip:** `FaultScenario::LowTip` (window open) → `FeeTooLow` → `Retry { refresh_blockhash:
  false, recalc_tip: true }` (raise the tip; the blockhash is still valid).

In the live run (Phase 8) the deterministic `decide_retry` policy is replaced by the AI agent's
reasoned decision over the same signals — same loop, with a visible reasoning trace.

## Methodology notes
Develop + inject on testnet/devnet; the explorer-verifiable proof run is on mainnet. Each run is
timestamped and persisted; decision traces are exported alongside the lifecycle log.

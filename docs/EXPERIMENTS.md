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

Before any live run, the chaos → classify → recover loop is proven deterministically in
`prometheon-faultinject/tests/chaos_loop.rs` (both tests green in CI):

- **Blockhash expiry (mandatory).** `FaultScenario::BlockhashExpiry.apply(signals)` sets
  `block_height_exceeded = true`, `blockhash_valid = Some(false)`, `landed = false`. `classify`
  returns `class = ExpiredBlockhash`, `confidence = 0.92`, `grade = Observable`, `retryable = true`.
  `decide_retry(ExpiredBlockhash, attempt 1, max 3)` returns
  `Retry { refresh_blockhash: true, recalc_tip: true, next_attempt: 2 }` — refresh the blockhash,
  re-price the tip from live data, resubmit.
- **Low tip (window open).** `FaultScenario::LowTip { tip_lamports: 500 }` with `blockhash_valid =
  Some(true)`. `classify` returns `class = FeeTooLow`, `retryable = true`. `decide_retry` returns
  `Retry { refresh_blockhash: false, recalc_tip: true }` — raise the tip only; the blockhash is
  still valid, so it is **not** refreshed.

## Submit → telemetry → export pipeline (proven without network)

`prometheon-core/tests/proof_pipeline.rs` drives the integrated proof driver
(`proof_run::track_and_emit`) over a capturing sink with scripted stream events, then runs the
captured telemetry through the same assembler `export-log` uses (`export::build_log`). It asserts a
**populated** lifecycle log: 12 bundles, 10 landed (`submitted→processed→confirmed→finalized` with
real slots + latencies), and 2 deliberately-injected, correctly-classified failures
(`fee_too_low`, `expired_blockhash`). This regression-guards the wiring whose absence previously made
the exported log come out empty.

In the live run (Tier 5) the deterministic `decide_retry` policy is replaced by the AI agent's
reasoned decision over the same signals — same loop, with a visible reasoning trace — and the proof
driver injects the same faults against mainnet to produce the explorer-verifiable log.

## Methodology notes
Develop + inject on testnet/devnet; the explorer-verifiable proof run is on mainnet. Each run is
timestamped and persisted; decision traces are exported alongside the lifecycle log.

//! The submission saga — the autonomous bundle loop's brain.
//!
//! One *attempt* runs: decide tip (AI strategist) → assemble + sign → submit → track lifecycle via
//! the stream → observe an outcome. The *saga* spans attempts: on a non-landing it classifies the
//! failure and asks the retry policy whether/how to retry (refresh blockhash, re-price the tip),
//! up to a cap. This module holds the **pure decision core** of that saga so it's exhaustively
//! unit-testable; the live I/O (RPC blockhash, Jito submit, stream correlation) is a thin driver
//! layered on top in the proof run, where it can be exercised against mainnet.
//!
//! The decision core deliberately *reuses* the already-tested [`classify`] and [`decide_retry`] —
//! the saga is the place those compose into autonomous behaviour, with the AI agent layering
//! visible reasoning on top of (and able to override) the safe default policy.

use prometheon_failure::{classify, FailureClass, FailureSignals};
use prometheon_netmodel::HealthSnapshot;
use prometheon_retry::{decide_retry, RetryAction};
use prometheon_telemetry::Decision;
use prometheon_types::Slot;
use serde_json::{json, Value};

/// The observed outcome of one submission attempt (from the stream + status polls).
#[derive(Debug, Clone)]
pub enum SubmissionOutcome {
    /// The bundle landed in `slot`.
    Landed { slot: Slot },
    /// The bundle did not land; `signals` capture why (fed to the classifier).
    Failed { signals: FailureSignals },
}

impl SubmissionOutcome {
    pub fn landed(&self) -> bool {
        matches!(self, SubmissionOutcome::Landed { .. })
    }
}

/// What the saga should do after an attempt finishes.
#[derive(Debug, Clone, PartialEq)]
pub enum SagaAction {
    /// (Re)submit as attempt `next_attempt`, refreshing/re-pricing as indicated.
    Submit {
        next_attempt: u32,
        refresh_blockhash: bool,
        recalc_tip: bool,
    },
    /// Landed — success; stop.
    Landed { slot: Slot },
    /// Give up. `class` is the failure classification that led here (if any).
    Abandon {
        reason: String,
        class: Option<FailureClass>,
    },
}

/// Decide the next saga action given the just-finished attempt's `outcome`.
///
/// `attempt` is the 1-indexed attempt that just ran; `max_attempts` is the cap. On a non-landing
/// this classifies the signals, then defers to [`decide_retry`] for the safe default policy
/// (expiry/timeout → refresh blockhash; always re-price; non-retryable classes → abandon).
pub fn next_saga_action(
    outcome: &SubmissionOutcome,
    attempt: u32,
    max_attempts: u32,
) -> SagaAction {
    match outcome {
        SubmissionOutcome::Landed { slot } => SagaAction::Landed { slot: *slot },
        SubmissionOutcome::Failed { signals } => {
            let class = classify(signals).class;
            match decide_retry(class, attempt, max_attempts) {
                RetryAction::Retry {
                    refresh_blockhash,
                    recalc_tip,
                    next_attempt,
                } => SagaAction::Submit {
                    next_attempt,
                    refresh_blockhash,
                    recalc_tip,
                },
                RetryAction::Abandon { reason } => SagaAction::Abandon {
                    reason,
                    class: Some(class),
                },
            }
        }
    }
}

/// Build the context the AI tip strategist reasons over.
///
/// Keys are camelCase to match the agent's prompt + `MockProvider` (`congestionScore`,
/// `tipFloorP50Lamports`, …). The agent echoes this object back as `inputs_considered`, so it also
/// becomes the audit trail of exactly what the model saw.
pub fn tip_context(
    snapshot: &HealthSnapshot,
    recent_failures: u32,
    last_tip_lamports: Option<u64>,
) -> Value {
    json!({
        "congestionScore": snapshot.congestion_score,
        "slotStabilityScore": snapshot.slot_stability_score,
        "bundleLandingProbability": snapshot.bundle_landing_probability,
        "tipFloorP50Lamports": snapshot.tip_floor_lamports,
        "avgConfirmedLatencyMs": snapshot.avg_confirmed_latency_ms,
        "recentFailures": recent_failures,
        "lastTipLamports": last_tip_lamports,
    })
}

/// Extract the chosen tip (lamports) from an AI tip decision's `after` state, if present.
///
/// The agent's tip decision carries the concrete choice in `after.tip`; this reads it back so the
/// deterministic core can act on the model's number while keeping the full reasoning trace.
pub fn tip_from_decision(decision: &Decision) -> Option<u64> {
    decision.after.as_ref()?.get("tip")?.as_u64()
}

/// Resolve the tip to actually use: prefer the AI decision's number; fall back to `fallback_lamports`
/// (a deterministic live-data computation) when the agent is unavailable or returned no number.
pub fn resolve_tip(decision: Option<&Decision>, fallback_lamports: u64) -> u64 {
    decision
        .and_then(tip_from_decision)
        .unwrap_or(fallback_lamports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use prometheon_failure::OnChainError;
    use prometheon_faultinject::{normal_signals, FaultScenario};
    use prometheon_telemetry::DecisionType;

    const MAX: u32 = 3;

    fn expiry_signals() -> FailureSignals {
        let mut s = normal_signals();
        FaultScenario::BlockhashExpiry.apply(&mut s);
        s
    }

    fn low_tip_signals() -> FailureSignals {
        let mut s = normal_signals();
        FaultScenario::LowTip {
            tip_lamports: 1_000,
        }
        .apply(&mut s);
        s
    }

    #[test]
    fn landing_finishes_the_saga() {
        let action = next_saga_action(&SubmissionOutcome::Landed { slot: 42 }, 1, MAX);
        assert_eq!(action, SagaAction::Landed { slot: 42 });
    }

    #[test]
    fn full_loop_expiry_then_low_tip_then_land() {
        // Attempt 1 fails on blockhash expiry → retry with a fresh blockhash + re-priced tip.
        let a1 = next_saga_action(
            &SubmissionOutcome::Failed {
                signals: expiry_signals(),
            },
            1,
            MAX,
        );
        assert_eq!(
            a1,
            SagaAction::Submit {
                next_attempt: 2,
                refresh_blockhash: true,
                recalc_tip: true,
            }
        );

        // Attempt 2 fails on too-low tip → retry, NO blockhash refresh (window still valid), re-price.
        let a2 = next_saga_action(
            &SubmissionOutcome::Failed {
                signals: low_tip_signals(),
            },
            2,
            MAX,
        );
        assert_eq!(
            a2,
            SagaAction::Submit {
                next_attempt: 3,
                refresh_blockhash: false,
                recalc_tip: true,
            }
        );

        // Attempt 3 lands.
        let a3 = next_saga_action(&SubmissionOutcome::Landed { slot: 99 }, 3, MAX);
        assert_eq!(a3, SagaAction::Landed { slot: 99 });
    }

    #[test]
    fn attempt_cap_abandons() {
        // A retryable failure at the cap abandons rather than looping forever.
        let action = next_saga_action(
            &SubmissionOutcome::Failed {
                signals: expiry_signals(),
            },
            MAX,
            MAX,
        );
        match action {
            SagaAction::Abandon { class, .. } => {
                assert_eq!(class, Some(FailureClass::ExpiredBlockhash));
            }
            other => panic!("expected Abandon, got {other:?}"),
        }
    }

    #[test]
    fn non_retryable_failure_abandons_immediately() {
        // A landed-but-reverted compute-exceeded error is not retryable as-is.
        let mut signals = normal_signals();
        signals.landed = true;
        signals.on_chain_error = Some(OnChainError::ComputeBudgetExceeded);
        let action = next_saga_action(&SubmissionOutcome::Failed { signals }, 1, MAX);
        match action {
            SagaAction::Abandon { class, .. } => {
                assert_eq!(class, Some(FailureClass::ComputeExceeded));
            }
            other => panic!("expected Abandon, got {other:?}"),
        }
    }

    #[test]
    fn tip_context_uses_agent_keys() {
        let snap = HealthSnapshot {
            ts: Utc::now(),
            congestion_score: 0.74,
            slot_stability_score: 0.9,
            bundle_landing_probability: 0.8,
            retry_success_rate: 0.5,
            tip_efficiency_ratio: 0.0,
            cost_per_successful_landing: None,
            avg_confirmed_latency_ms: Some(850.0),
            confirm_latency_variance_ms: None,
            tip_floor_lamports: 14_200,
        };
        let ctx = tip_context(&snap, 2, Some(12_000));
        assert_eq!(ctx["congestionScore"], 0.74);
        assert_eq!(ctx["tipFloorP50Lamports"], 14_200);
        assert_eq!(ctx["recentFailures"], 2);
        assert_eq!(ctx["lastTipLamports"], 12_000);
    }

    fn tip_decision(after: Option<Value>) -> Decision {
        Decision {
            decision_type: DecisionType::Tip,
            action: "tip".into(),
            reasoning: "r".into(),
            confidence: 0.7,
            inputs_considered: json!({}),
            before: None,
            after,
            provider: "mock".into(),
            latency_ms: 1,
            ts: Utc::now(),
        }
    }

    #[test]
    fn tip_from_decision_reads_after_tip() {
        assert_eq!(
            tip_from_decision(&tip_decision(Some(json!({ "tip": 18_000 })))),
            Some(18_000)
        );
        assert_eq!(tip_from_decision(&tip_decision(None)), None);
        assert_eq!(
            tip_from_decision(&tip_decision(Some(json!({ "other": 1 })))),
            None
        );
    }

    #[test]
    fn resolve_tip_prefers_agent_then_falls_back() {
        let with = tip_decision(Some(json!({ "tip": 18_000 })));
        assert_eq!(resolve_tip(Some(&with), 9_999), 18_000);
        let without = tip_decision(None);
        assert_eq!(resolve_tip(Some(&without), 9_999), 9_999);
        assert_eq!(resolve_tip(None, 9_999), 9_999);
    }
}

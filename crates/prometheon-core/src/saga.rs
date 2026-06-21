//! The autonomous submission saga — **the AI in the loop**.
//!
//! This is where the agent actually *owns* an operational decision in the running system (the
//! bounty's headline: "Autonomous Retry with Fault Injection"). For each bundle the saga:
//!   1. asks the AI for a **tip** decision, submits attempt 1, and emits the `Decision` + `Bundle`;
//!   2. tracks every attempt on the one shared stream (`PendingBundles`), emitting `Lifecycle`;
//!   3. on a non-landing, **classifies** the failure, asks the AI for a **retry** decision
//!      (refresh blockhash? new tip?), emits it, then resubmits the next attempt — until it lands or
//!      the attempt cap / a non-retryable class abandons it.
//!
//! The agent *proposes*; the deterministic core *enforces safety* (a forced blockhash refresh on
//! expiry is never downgraded; the tip is clamped by the submitter). Live I/O is behind two traits
//! ([`DecisionSource`], [`Submitter`]) so the entire AI-driven loop — including recovery — is
//! exercised with **no network** in `tests/saga_pipeline.rs`; the live binary plugs in the NATS bus
//! and the real bundle submitter.

use std::collections::{HashMap, HashSet};

use chrono::Utc;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio::time::Instant;

use prometheon_failure::FailureClass;
use prometheon_faultinject::FaultScenario;
use prometheon_ingest::IngestMessage;
use prometheon_retry::policy::{decide_retry, RetryAction};
use prometheon_telemetry::{Decision, DecisionType, TelemetryEvent};
use prometheon_types::Slot;

use crate::proof_run::{
    bundle_submitted_event, classify_nonland, failure_event, lifecycle_event, ProofSummary,
    SubmittedBundle,
};
use crate::sinks::EventSink;
use crate::submission::tip_from_decision;

/// Source of AI decisions. Live impl wraps `TelemetryBus::request_decision` (returns `None` on
/// timeout/error so the saga degrades gracefully); tests return scripted decisions.
pub trait DecisionSource {
    fn decide(
        &self,
        decision_type: DecisionType,
        context: Value,
    ) -> impl std::future::Future<Output = Option<Decision>> + Send;
}

/// What to build + send for one attempt.
#[derive(Debug, Clone)]
pub struct AttemptSpec {
    pub base_id: String,
    pub attempt_no: u32,
    /// Tip chosen by the AI (or `None` → the submitter computes it from the live floor).
    pub tip_lamports: Option<u64>,
    /// Fetch a fresh blockhash before resubmitting (set by the retry decision / safety policy).
    pub refresh_blockhash: bool,
    /// Deliberate fault to inject (attempt 1 only; retries never inject).
    pub injected: Option<FaultScenario>,
}

/// Builds, signs, and submits one attempt, returning the tracked [`SubmittedBundle`]. Live impl runs
/// `prepare_attempt` + `send_bundle`; tests return a scripted bundle.
pub trait Submitter {
    fn submit(
        &self,
        spec: &AttemptSpec,
    ) -> impl std::future::Future<Output = anyhow::Result<SubmittedBundle>> + Send;
}

/// One logical bundle to attempt (with the context the AI tip decision reasons over).
#[derive(Debug, Clone)]
pub struct BaseBundle {
    pub base_id: String,
    pub injected: Option<FaultScenario>,
    /// camelCase context for the tip decision (congestion, tip floor, …) — echoed as `inputs_considered`.
    pub tip_context: Value,
}

/// Saga tuning.
#[derive(Debug, Clone, Copy)]
pub struct SagaConfig {
    pub max_attempts: u32,
    pub global_deadline: Instant,
}

/// The resolved retry action — the AI's choice, with safety invariants enforced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPlan {
    pub retry: bool,
    pub refresh_blockhash: bool,
    pub tip_lamports: Option<u64>,
    pub reason: String,
}

/// Combine the AI's retry decision with the deterministic safety policy: the agent drives the choice,
/// but a blockhash refresh forced by the failure class (expiry/timeout) is **never** downgraded, and
/// the attempt cap / non-retryable classes still abandon. The AI's tip flows through (clamped later).
pub fn resolve_retry(
    class: FailureClass,
    attempt: u32,
    max_attempts: u32,
    ai: Option<&Decision>,
) -> RetryPlan {
    match decide_retry(class, attempt, max_attempts) {
        RetryAction::Abandon { reason } => RetryPlan {
            retry: false,
            refresh_blockhash: false,
            tip_lamports: None,
            reason: ai
                .map(|d| d.reasoning.clone())
                .filter(|r| !r.is_empty())
                .unwrap_or(reason),
        },
        RetryAction::Retry {
            refresh_blockhash,
            recalc_tip,
            ..
        } => {
            let ai_refresh = ai
                .and_then(|d| d.after.as_ref())
                .and_then(|a| a.get("refresh_blockhash"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let reason = ai
                .map(|d| d.reasoning.clone())
                .filter(|r| !r.is_empty())
                .unwrap_or_else(|| {
                    format!("policy: refresh={refresh_blockhash}, recalc={recalc_tip}")
                });
            RetryPlan {
                retry: true,
                // safety: keep a class-forced refresh even if the model didn't ask for one.
                refresh_blockhash: refresh_blockhash || ai_refresh,
                tip_lamports: ai.and_then(tip_from_decision),
                reason,
            }
        }
    }
}

/// The context the AI retry decision reasons over.
pub fn retry_context(base_ctx: &Value, class: FailureClass, attempt: u32, last_tip: u64) -> Value {
    json!({
        "failureClass": format!("{class:?}"),
        "attempt": attempt,
        "lastTipLamports": last_tip,
        "congestionScore": base_ctx.get("congestionScore"),
        "tipFloorP50Lamports": base_ctx.get("tipFloorP50Lamports"),
        "slotStabilityScore": base_ctx.get("slotStabilityScore"),
    })
}

/// A visible "decision" trace for when the AI agent is unavailable — the deterministic policy is
/// still a decision, just not an LLM one. Keeps the timeline populated and honest.
fn fallback_decision(decision_type: DecisionType, context: Value, reasoning: String) -> Decision {
    Decision {
        decision_type,
        action: "policy fallback".into(),
        reasoning,
        confidence: 0.5,
        inputs_considered: context,
        before: None,
        after: None,
        provider: "policy-fallback".into(),
        latency_ms: 0,
        ts: Utc::now(),
    }
}

struct BaseState {
    ctx: Value,
    current_attempt_id: String,
    last_tip: u64,
    landed: bool, // reached confirmed (counted) — but keep draining until finalized for a full log
    done: bool,   // finalized or abandoned — no more work
}

struct Saga<'a, E, D, S> {
    sink: &'a E,
    decider: &'a D,
    submitter: &'a S,
    cfg: SagaConfig,
    pending: crate::proof::PendingBundles,
    attempts: HashMap<String, SubmittedBundle>,
    bases: HashMap<String, BaseState>,
    landed: HashSet<String>,
    failed: HashSet<String>,
    latest_slot: Slot,
    summary: ProofSummary,
}

impl<E: EventSink, D: DecisionSource, S: Submitter> Saga<'_, E, D, S> {
    /// Emit a bundle's most-recently recorded lifecycle transition.
    async fn emit_last_lifecycle(&self, id: &str) {
        if let Some(pb) = self.pending.get(id) {
            if let Some(ev) = pb.lifecycle.events().last().cloned() {
                self.sink.emit(&lifecycle_event(id, ev)).await;
            }
        }
    }

    /// Build + submit one attempt, emit its `Bundle` + initial `Submitted` lifecycle, and track it.
    async fn launch(&mut self, spec: AttemptSpec) {
        match self.submitter.submit(&spec).await {
            Ok(sb) => {
                self.summary.total += 1;
                self.sink.emit(&bundle_submitted_event(&sb)).await;
                self.pending.track(
                    sb.bundle_id.clone(),
                    vec![sb.signature.clone()],
                    sb.tip_lamports,
                    sb.submitted_at,
                );
                self.emit_last_lifecycle(&sb.bundle_id).await;
                if let Some(b) = self.bases.get_mut(&sb.base_id) {
                    b.current_attempt_id = sb.bundle_id.clone();
                    b.last_tip = sb.tip_lamports;
                }
                self.attempts.insert(sb.bundle_id.clone(), sb);
            }
            Err(e) => {
                tracing::warn!(error = %e, base = %spec.base_id, "submit failed");
                if let Some(b) = self.bases.get_mut(&spec.base_id) {
                    if !b.done {
                        b.done = true;
                        self.summary.failed += 1;
                    }
                }
            }
        }
    }

    /// Classify the failure, ask the AI whether/how to retry, emit the decision, and either resubmit
    /// the next attempt or abandon the bundle.
    async fn fail_and_retry(&mut self, attempt_id: &str) {
        if self.failed.contains(attempt_id) {
            return;
        }
        let Some(sb) = self.attempts.get(attempt_id).cloned() else {
            return;
        };
        if self.bases.get(&sb.base_id).map(|b| b.done).unwrap_or(true) {
            return;
        }
        self.failed.insert(attempt_id.to_string());
        self.summary.failed += 1;

        let classification = classify_nonland(&sb);
        let class = classification.class;
        self.sink
            .emit(&failure_event(attempt_id, classification))
            .await;

        let base_ctx = self
            .bases
            .get(&sb.base_id)
            .map(|b| b.ctx.clone())
            .unwrap_or(json!({}));
        let rctx = retry_context(&base_ctx, class, sb.attempt_no, sb.tip_lamports);
        let ai = self.decider.decide(DecisionType::Retry, rctx.clone()).await;
        let decision = ai.unwrap_or_else(|| {
            fallback_decision(
                DecisionType::Retry,
                rctx,
                format!("agent unavailable; applying retry policy for {class:?}"),
            )
        });
        self.sink
            .emit(&TelemetryEvent::Decision(decision.clone()))
            .await;

        let plan = resolve_retry(class, sb.attempt_no, self.cfg.max_attempts, Some(&decision));
        if !plan.retry {
            if let Some(b) = self.bases.get_mut(&sb.base_id) {
                b.done = true;
            }
            tracing::info!(base = %sb.base_id, reason = %plan.reason, "abandoned");
            return;
        }
        self.launch(AttemptSpec {
            base_id: sb.base_id.clone(),
            attempt_no: sb.attempt_no + 1,
            tip_lamports: plan.tip_lamports,
            refresh_blockhash: plan.refresh_blockhash,
            injected: None,
        })
        .await;
    }

    /// After any stream advance: count attempts that reached `confirmed` as landed, mark a base
    /// `done` once its attempt is `finalized` (so the log captures the full progression), and fail
    /// attempts past their give-up watermark (driving autonomous retry).
    async fn reconcile(&mut self) {
        // Confirmations → landed (counted once; the base keeps draining toward finalized).
        let newly_landed: Vec<String> = self
            .attempts
            .keys()
            .filter(|id| !self.landed.contains(*id))
            .filter(|id| {
                self.pending
                    .get(id)
                    .map(|pb| pb.lifecycle.reached_confirmed())
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
        for id in newly_landed {
            self.landed.insert(id.clone());
            self.summary.landed += 1;
            if let Some(sb) = self.attempts.get(&id) {
                if let Some(b) = self.bases.get_mut(&sb.base_id) {
                    b.landed = true;
                }
            }
        }

        // Finalizations → the base is done.
        let finalized: Vec<String> = self
            .bases
            .iter()
            .filter(|(_, b)| !b.done)
            .filter(|(_, b)| {
                self.pending
                    .get(&b.current_attempt_id)
                    .map(|pb| pb.lifecycle.is_success())
                    .unwrap_or(false)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in finalized {
            if let Some(b) = self.bases.get_mut(&id) {
                b.done = true;
            }
        }

        // Give-up deadlines → fail (and maybe retry) — but never give up on a bundle that landed.
        let expired: Vec<String> = self
            .bases
            .values()
            .filter(|b| !b.done && !b.landed)
            .filter_map(|b| {
                let sb = self.attempts.get(&b.current_attempt_id)?;
                let past = sb
                    .deadline_slot
                    .map(|d| self.latest_slot >= d)
                    .unwrap_or(false);
                (past && !self.failed.contains(&b.current_attempt_id))
                    .then(|| b.current_attempt_id.clone())
            })
            .collect();
        for id in expired {
            self.fail_and_retry(&id).await;
        }
    }

    fn all_done(&self) -> bool {
        !self.bases.is_empty() && self.bases.values().all(|b| b.done)
    }

    async fn run(&mut self, bases: Vec<BaseBundle>, rx: &mut mpsc::Receiver<IngestMessage>) {
        // Submit attempt 1 for every base, each with a fresh AI tip decision.
        for base in bases {
            let ai = self
                .decider
                .decide(DecisionType::Tip, base.tip_context.clone())
                .await;
            let decision = ai.unwrap_or_else(|| {
                fallback_decision(
                    DecisionType::Tip,
                    base.tip_context.clone(),
                    "agent unavailable; tip derived from live floor + congestion".into(),
                )
            });
            self.sink
                .emit(&TelemetryEvent::Decision(decision.clone()))
                .await;
            let tip = tip_from_decision(&decision);
            self.bases.insert(
                base.base_id.clone(),
                BaseState {
                    ctx: base.tip_context,
                    current_attempt_id: String::new(),
                    last_tip: 0,
                    landed: false,
                    done: false,
                },
            );
            self.launch(AttemptSpec {
                base_id: base.base_id,
                attempt_no: 1,
                tip_lamports: tip,
                refresh_blockhash: false,
                injected: base.injected,
            })
            .await;
        }

        // Drain the shared stream until everything is done (finalized/abandoned) or the deadline.
        loop {
            if self.all_done() {
                break;
            }
            let remaining = self
                .cfg
                .global_deadline
                .saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Some(IngestMessage::Transaction(tx))) => {
                    if let Some(c) =
                        self.pending
                            .on_tx_status(&tx.signature, tx.slot, tx.failed, tx.ts)
                    {
                        let id = c.bundle_id.clone();
                        self.emit_last_lifecycle(&id).await;
                        if tx.failed {
                            self.fail_and_retry(&id).await;
                        }
                    }
                    self.reconcile().await;
                }
                Ok(Some(IngestMessage::Slot { update, .. })) => {
                    self.latest_slot = self.latest_slot.max(update.slot);
                    for c in self
                        .pending
                        .on_slot_status(update.slot, update.status, update.ts)
                    {
                        self.emit_last_lifecycle(&c.bundle_id).await;
                    }
                    self.reconcile().await;
                }
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }

        // Sweep at the deadline: a bundle that reached confirmed but not finalized in time is still a
        // success; anything else still open is a failure (timeout/dropped).
        let landed_pending: Vec<String> = self
            .bases
            .iter()
            .filter(|(_, b)| !b.done && b.landed)
            .map(|(id, _)| id.clone())
            .collect();
        for id in landed_pending {
            if let Some(b) = self.bases.get_mut(&id) {
                b.done = true;
            }
        }
        let stuck: Vec<String> = self
            .bases
            .values()
            .filter(|b| !b.done && !self.failed.contains(&b.current_attempt_id))
            .map(|b| b.current_attempt_id.clone())
            .collect();
        for id in stuck {
            self.fail_and_retry(&id).await;
        }
    }
}

/// Run the autonomous saga over `bases`, emitting all telemetry through `sink`, asking `decider` for
/// AI tip/retry decisions, and submitting via `submitter`. Returns per-attempt outcome counts.
pub async fn run_saga<E: EventSink, D: DecisionSource, S: Submitter>(
    sink: &E,
    decider: &D,
    submitter: &S,
    bases: Vec<BaseBundle>,
    rx: &mut mpsc::Receiver<IngestMessage>,
    cfg: SagaConfig,
) -> ProofSummary {
    let mut saga = Saga {
        sink,
        decider,
        submitter,
        cfg,
        pending: crate::proof::PendingBundles::new(),
        attempts: HashMap::new(),
        bases: HashMap::new(),
        landed: HashSet::new(),
        failed: HashSet::new(),
        latest_slot: 0,
        summary: ProofSummary::default(),
    };
    saga.run(bases, rx).await;
    saga.summary
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ai_retry(refresh: bool, tip: u64) -> Decision {
        Decision {
            decision_type: DecisionType::Retry,
            action: "retry".into(),
            reasoning: "blockhash expired; refresh and re-price".into(),
            confidence: 0.9,
            inputs_considered: json!({}),
            before: None,
            after: Some(json!({ "refresh_blockhash": refresh, "tip": tip })),
            provider: "anthropic".into(),
            latency_ms: 700,
            ts: Utc::now(),
        }
    }

    #[test]
    fn resolve_retry_honors_ai_but_enforces_safety() {
        // AI says don't refresh, but an expired blockhash forces a refresh anyway (safety).
        let d = Decision {
            after: Some(json!({ "refresh_blockhash": false, "tip": 30_000 })),
            ..ai_retry(false, 30_000)
        };
        let plan = resolve_retry(FailureClass::ExpiredBlockhash, 1, 3, Some(&d));
        assert!(plan.retry);
        assert!(
            plan.refresh_blockhash,
            "expiry must force a refresh regardless of the model"
        );
        assert_eq!(plan.tip_lamports, Some(30_000));

        // Fee-too-low: blockhash still valid, only re-price; AI tip flows through.
        let plan = resolve_retry(
            FailureClass::FeeTooLow,
            1,
            3,
            Some(&ai_retry(false, 25_000)),
        );
        assert!(plan.retry);
        assert!(!plan.refresh_blockhash);
        assert_eq!(plan.tip_lamports, Some(25_000));
    }

    #[test]
    fn resolve_retry_abandons_at_cap_and_for_nonretryable() {
        assert!(!resolve_retry(FailureClass::ExpiredBlockhash, 3, 3, None).retry); // cap
        assert!(!resolve_retry(FailureClass::ComputeExceeded, 1, 3, None).retry);
        // non-retryable
    }

    #[test]
    fn retry_context_carries_failure_and_market_state() {
        let base = json!({ "congestionScore": 0.7, "tipFloorP50Lamports": 14_200, "slotStabilityScore": 0.9 });
        let ctx = retry_context(&base, FailureClass::ExpiredBlockhash, 1, 12_000);
        assert_eq!(ctx["failureClass"], "ExpiredBlockhash");
        assert_eq!(ctx["attempt"], 1);
        assert_eq!(ctx["lastTipLamports"], 12_000);
        assert_eq!(ctx["tipFloorP50Lamports"], 14_200);
    }
}

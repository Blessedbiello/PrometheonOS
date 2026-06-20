//! Per-submission transaction lifecycle state machine.
//!
//! Tracks one bundle/transaction attempt across commitment levels, capturing the slot, timestamp,
//! and inter-stage latency the bounty requires. Driven primarily by the Yellowstone stream (slot
//! status + tx status), with RPC as a cross-check.
//!
//! The machine is deliberately strict: illegal transitions are rejected (not silently applied) so
//! the recorded history is always a valid path. Retry/abandon across attempts is a *saga*-level
//! concern owned by the retry orchestrator (Phase 6), not this per-attempt machine.

use chrono::{DateTime, Utc};
use prometheon_types::Slot;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A stage in one submission's lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStage {
    /// Bundle accepted by the Block Engine (we have a bundle id).
    Submitted,
    /// Seen in a block (slot `processed`); not yet voted — fork-revertible.
    Processed,
    /// Slot reached `confirmed` (supermajority optimistic vote).
    Confirmed,
    /// Slot reached `finalized` (rooted, irreversible). Terminal success.
    Finalized,
    /// Failed: on-chain error, bundle rejected, or the processed slot went dead. Terminal.
    Failed,
    /// Blockhash expired before inclusion (block height passed `lastValidBlockHeight`). Terminal.
    Expired,
    /// Dropped: leader skipped / no landing in the validity window. Terminal.
    Dropped,
}

impl LifecycleStage {
    /// Whether the stage is terminal (no further transitions).
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            LifecycleStage::Finalized
                | LifecycleStage::Failed
                | LifecycleStage::Expired
                | LifecycleStage::Dropped
        )
    }

    /// Whether `to` is a legal next stage from `self`.
    ///
    /// Forward-skips are allowed (`Submitted → Confirmed`, `Processed → Finalized`) because the
    /// stream can deliver a later commitment status without our having observed the intermediate one
    /// (e.g. a dropped `confirmed` slot-status): we should still advance rather than strand the
    /// bundle. Backward and otherwise-illegal transitions remain rejected.
    pub fn can_transition_to(self, to: LifecycleStage) -> bool {
        use LifecycleStage::*;
        matches!(
            (self, to),
            (Submitted, Processed)
                | (Submitted, Confirmed)
                | (Submitted, Finalized)
                | (Submitted, Failed)
                | (Submitted, Expired)
                | (Submitted, Dropped)
                | (Processed, Confirmed)
                | (Processed, Finalized)
                | (Processed, Failed)
                | (Confirmed, Finalized)
                | (Confirmed, Failed)
        )
    }
}

/// The result of attempting a transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionOutcome {
    /// Recorded a new event and advanced the stage.
    Advanced,
    /// The requested stage equals the current stage — ignored (e.g. a resent stream update).
    Redundant,
    /// The transition is not allowed from the current stage; nothing changed.
    Illegal {
        from: LifecycleStage,
        to: LifecycleStage,
    },
}

/// One recorded lifecycle transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LifecycleEvent {
    pub stage: LifecycleStage,
    /// Slot associated with the stage (absent for some failures like pre-inclusion expiry).
    pub slot: Option<Slot>,
    pub ts: DateTime<Utc>,
    /// Milliseconds since the previous recorded event (`None` for the first, Submitted).
    pub delta_ms_from_prev: Option<i64>,
}

/// Tracks one submission attempt, keyed by its bundle id / signature.
#[derive(Debug, Clone)]
pub struct TransactionLifecycle {
    id: String,
    current: LifecycleStage,
    events: Vec<LifecycleEvent>,
    submitted_at: DateTime<Utc>,
    processed_at: Option<DateTime<Utc>>,
    confirmed_at: Option<DateTime<Utc>>,
    finalized_at: Option<DateTime<Utc>>,
}

impl TransactionLifecycle {
    /// Start a lifecycle in [`LifecycleStage::Submitted`] at `submitted_at`.
    pub fn new(id: impl Into<String>, submitted_at: DateTime<Utc>) -> Self {
        Self {
            id: id.into(),
            current: LifecycleStage::Submitted,
            events: vec![LifecycleEvent {
                stage: LifecycleStage::Submitted,
                slot: None,
                ts: submitted_at,
                delta_ms_from_prev: None,
            }],
            submitted_at,
            processed_at: None,
            confirmed_at: None,
            finalized_at: None,
        }
    }

    /// The bundle id / signature this lifecycle tracks.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The current stage.
    pub fn current_stage(&self) -> LifecycleStage {
        self.current
    }

    /// Recorded events in order.
    pub fn events(&self) -> &[LifecycleEvent] {
        &self.events
    }

    /// Whether the lifecycle has reached a terminal stage.
    pub fn is_terminal(&self) -> bool {
        self.current.is_terminal()
    }

    /// Whether the lifecycle ended in success (finalized).
    pub fn is_success(&self) -> bool {
        self.current == LifecycleStage::Finalized
    }

    /// Whether the bundle reached at least `confirmed` — the bar for a stream-confirmed landing
    /// (a `processed`-only bundle is in a block but fork-revertible, so it does not yet count).
    pub fn reached_confirmed(&self) -> bool {
        matches!(
            self.current,
            LifecycleStage::Confirmed | LifecycleStage::Finalized
        )
    }

    /// Attempt to advance to `to` at slot/time `ts`. See [`TransitionOutcome`].
    pub fn advance(
        &mut self,
        to: LifecycleStage,
        slot: Option<Slot>,
        ts: DateTime<Utc>,
    ) -> TransitionOutcome {
        if to == self.current {
            return TransitionOutcome::Redundant;
        }
        if !self.current.can_transition_to(to) {
            return TransitionOutcome::Illegal {
                from: self.current,
                to,
            };
        }

        let delta = self
            .events
            .last()
            .map(|prev| (ts - prev.ts).num_milliseconds());
        self.events.push(LifecycleEvent {
            stage: to,
            slot,
            ts,
            delta_ms_from_prev: delta,
        });
        self.current = to;
        match to {
            LifecycleStage::Processed => self.processed_at = Some(ts),
            LifecycleStage::Confirmed => self.confirmed_at = Some(ts),
            LifecycleStage::Finalized => self.finalized_at = Some(ts),
            _ => {}
        }
        TransitionOutcome::Advanced
    }

    /// submit → processed latency (ms).
    pub fn processed_latency_ms(&self) -> Option<i64> {
        self.processed_at
            .map(|t| (t - self.submitted_at).num_milliseconds())
    }

    /// submit → confirmed latency (ms).
    pub fn confirmed_latency_ms(&self) -> Option<i64> {
        self.confirmed_at
            .map(|t| (t - self.submitted_at).num_milliseconds())
    }

    /// submit → finalized latency (ms).
    pub fn finalized_latency_ms(&self) -> Option<i64> {
        self.finalized_at
            .map(|t| (t - self.submitted_at).num_milliseconds())
    }

    /// processed → confirmed delta (ms) — the consensus-health signal (README Q1).
    pub fn processed_to_confirmed_ms(&self) -> Option<i64> {
        match (self.processed_at, self.confirmed_at) {
            (Some(p), Some(c)) => Some((c - p).num_milliseconds()),
            _ => None,
        }
    }
}

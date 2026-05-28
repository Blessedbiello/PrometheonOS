//! The composed network-health model.
//!
//! Aggregates recorded slot/lifecycle/tip events into a [`HealthSnapshot`] — the
//! `telemetry.health` payload the AI strategist consumes and the dashboard renders. Counters are
//! cumulative over the model's lifetime; latencies use a rolling window. Per-transaction
//! `expiry_risk` stays a standalone function ([`crate::metrics::expiry_risk_score`]) since it
//! depends on a specific tx's remaining blocks.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::metrics::{
    bundle_landing_probability, congestion_score, cost_per_successful_landing, retry_success_rate,
    slot_stability_score, tip_efficiency_ratio, CongestionWeights,
};
use crate::window::RollingWindow;

/// A point-in-time network-health + execution-quality snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthSnapshot {
    pub ts: DateTime<Utc>,
    pub congestion_score: f64,
    pub slot_stability_score: f64,
    pub bundle_landing_probability: f64,
    pub retry_success_rate: f64,
    pub tip_efficiency_ratio: f64,
    pub cost_per_successful_landing: Option<f64>,
    pub avg_confirmed_latency_ms: Option<f64>,
    pub confirm_latency_variance_ms: Option<f64>,
    pub tip_floor_lamports: u64,
}

/// Accumulates events and produces [`HealthSnapshot`]s.
#[derive(Debug, Clone)]
pub struct NetworkHealthModel {
    weights: CongestionWeights,
    slots_produced: u64,
    slots_skipped: u64,
    submitted: u64,
    landed: u64,
    total_tip_lamports: u64,
    total_cost_lamports: u64,
    retries_total: u64,
    retries_landed: u64,
    tip_floor_lamports: u64,
    confirmed_latency: RollingWindow,
}

impl NetworkHealthModel {
    /// Create a model with a rolling latency window of `latency_window` samples.
    pub fn new(latency_window: usize) -> Self {
        Self {
            weights: CongestionWeights::default(),
            slots_produced: 0,
            slots_skipped: 0,
            submitted: 0,
            landed: 0,
            total_tip_lamports: 0,
            total_cost_lamports: 0,
            retries_total: 0,
            retries_landed: 0,
            tip_floor_lamports: 0,
            confirmed_latency: RollingWindow::new(latency_window),
        }
    }

    /// Record a scheduled slot outcome: produced a block, or skipped.
    pub fn record_slot(&mut self, produced: bool) {
        if produced {
            self.slots_produced += 1;
        } else {
            self.slots_skipped += 1;
        }
    }

    /// Record a bundle submission.
    pub fn record_submission(&mut self) {
        self.submitted += 1;
    }

    /// Record a successful landing with its tip and total cost (tip + fees) in lamports.
    pub fn record_landing(&mut self, tip_lamports: u64, total_cost_lamports: u64) {
        self.landed += 1;
        self.total_tip_lamports += tip_lamports;
        self.total_cost_lamports += total_cost_lamports;
    }

    /// Record a confirm latency sample (submit→confirmed, ms).
    pub fn record_confirmed_latency_ms(&mut self, ms: f64) {
        self.confirmed_latency.push(ms);
    }

    /// Record a retry outcome (whether the retry subsequently landed).
    pub fn record_retry(&mut self, landed: bool) {
        self.retries_total += 1;
        if landed {
            self.retries_landed += 1;
        }
    }

    /// Update the latest tip-floor reading (lamports) used for congestion.
    pub fn set_tip_floor_lamports(&mut self, lamports: u64) {
        self.tip_floor_lamports = lamports;
    }

    /// Skip rate over observed slots, `[0,1]`.
    fn skip_rate(&self) -> f64 {
        let total = self.slots_produced + self.slots_skipped;
        if total == 0 {
            0.0
        } else {
            self.slots_skipped as f64 / total as f64
        }
    }

    /// Produce a snapshot at time `ts`.
    pub fn snapshot(&self, ts: DateTime<Utc>) -> HealthSnapshot {
        let avg_confirmed = self.confirmed_latency.mean();
        let congestion = congestion_score(
            self.skip_rate(),
            avg_confirmed.unwrap_or(0.0),
            self.tip_floor_lamports,
            &self.weights,
        );
        HealthSnapshot {
            ts,
            congestion_score: congestion,
            slot_stability_score: slot_stability_score(self.slots_produced, self.slots_skipped),
            bundle_landing_probability: bundle_landing_probability(self.landed, self.submitted),
            retry_success_rate: retry_success_rate(self.retries_landed, self.retries_total),
            tip_efficiency_ratio: tip_efficiency_ratio(self.landed, self.total_tip_lamports),
            cost_per_successful_landing: cost_per_successful_landing(
                self.total_cost_lamports,
                self.landed,
            ),
            avg_confirmed_latency_ms: avg_confirmed,
            confirm_latency_variance_ms: self.confirmed_latency.variance(),
            tip_floor_lamports: self.tip_floor_lamports,
        }
    }
}

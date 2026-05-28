//! `prometheon-faultinject`
//!
//! Deliberate chaos: deterministic scenarios that perturb the inputs the engine reasons over, so we
//! can observe how the failure classifier and AI strategist adapt. Each scenario documents a
//! hypothesis and produces telemetry we capture in `docs/EXPERIMENTS.md`.
//!
//! Pure (no network); the scenarios are applied to in-memory signals/config. Built test-first.

use prometheon_failure::FailureSignals;
use serde::{Deserialize, Serialize};

/// A baseline "successful, healthy" signal set that scenarios perturb. Useful as a test fixture and
/// as the neutral state the injector starts from.
pub fn normal_signals() -> FailureSignals {
    FailureSignals {
        on_chain_error: None,
        bundle_status_failed: false,
        block_height_exceeded: false,
        blockhash_valid: Some(true),
        landed: true,
        tip_lamports: 50_000,
        tip_floor_p50_lamports: 20_000,
        leader_missed: false,
        slot_skipped: false,
        confirmation_timeout: false,
    }
}

/// A chaos scenario.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FaultScenario {
    /// Force a stale/expired blockhash (the mandatory scenario).
    BlockhashExpiry,
    /// Submit with a tip below the floor.
    LowTip { tip_lamports: u64 },
    /// Delay submission by `delay_ms`, increasing expiry risk.
    DelayedSubmission { delay_ms: u64 },
    /// Drop every `drop_every`-th stream event to simulate ingestion gaps.
    DroppedStreamEvents { drop_every: u32 },
    /// Override the congestion score fed to the model.
    Congestion { score: f64 },
}

impl FaultScenario {
    /// Apply a signal-perturbing scenario to `signals` (no-op for non-signal scenarios).
    pub fn apply(&self, signals: &mut FailureSignals) {
        match *self {
            FaultScenario::BlockhashExpiry => {
                signals.block_height_exceeded = true;
                signals.blockhash_valid = Some(false);
                signals.landed = false;
            }
            FaultScenario::LowTip { tip_lamports } => {
                signals.tip_lamports = tip_lamports;
                signals.landed = false;
            }
            FaultScenario::DelayedSubmission { .. }
            | FaultScenario::DroppedStreamEvents { .. }
            | FaultScenario::Congestion { .. } => {}
        }
    }

    /// The congestion score to inject into the health model, if this is a congestion scenario.
    pub fn congestion_override(&self) -> Option<f64> {
        match self {
            FaultScenario::Congestion { score } => Some(*score),
            _ => None,
        }
    }

    /// The artificial submission delay (ms), if this is a delay scenario.
    pub fn submission_delay_ms(&self) -> Option<u64> {
        match self {
            FaultScenario::DelayedSubmission { delay_ms } => Some(*delay_ms),
            _ => None,
        }
    }
}

/// Holds the active scenarios and applies cross-cutting effects (e.g. event dropping).
#[derive(Debug, Clone, Default)]
pub struct FaultInjector {
    scenarios: Vec<FaultScenario>,
}

impl FaultInjector {
    /// Create an injector with the given active scenarios.
    pub fn new(scenarios: Vec<FaultScenario>) -> Self {
        Self { scenarios }
    }

    /// The active scenarios.
    pub fn scenarios(&self) -> &[FaultScenario] {
        &self.scenarios
    }

    /// Whether the `event_index`-th (1-indexed) stream event should be dropped, per any active
    /// `DroppedStreamEvents` scenario.
    pub fn should_drop_event(&self, event_index: u64) -> bool {
        self.scenarios.iter().any(|s| match s {
            FaultScenario::DroppedStreamEvents { drop_every } if *drop_every > 0 => {
                event_index % *drop_every as u64 == 0
            }
            _ => false,
        })
    }

    /// Apply all signal-perturbing scenarios to `signals`.
    pub fn apply_signals(&self, signals: &mut FailureSignals) {
        for s in &self.scenarios {
            s.apply(signals);
        }
    }
}

//! The telemetry event envelope.
//!
//! [`TelemetryEvent`] is the single tagged enum that flows over NATS and into persistence. It wraps
//! the domain types from the engine crates and maps each variant to a stable NATS subject. JSON is
//! the wire format (shared with the TypeScript agent + dashboard via the generated `contracts/`).

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use prometheon_failure::FailureClassification;
use prometheon_lifecycle::lifecycle::LifecycleEvent;
use prometheon_netmodel::HealthSnapshot;
use prometheon_types::SlotUpdate;

use crate::decision::{Decision, DecisionType};

/// Phase of a bundle's submission, for the bundle telemetry stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BundlePhase {
    /// We sent the bundle to the Block Engine.
    Submitted,
    /// The Block Engine acknowledged it (we have a bundle id).
    Acked,
    /// A status poll updated the bundle's state.
    StatusUpdate,
}

/// A bundle telemetry event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BundleEvent {
    pub bundle_id: String,
    pub tip_lamports: u64,
    pub tip_account: String,
    pub region: String,
    pub signatures: Vec<String>,
    pub phase: BundlePhase,
    pub ts: DateTime<Utc>,
    /// The logical bundle this attempt belongs to — retries of the same bundle share a `base_id`. This
    /// lets the lifecycle-log export chain a failed attempt to its recovered resubmission (the AI
    /// recovery unit). Optional for backward compatibility with telemetry emitted before this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_id: Option<String>,
    /// 1-indexed attempt number within `base_id` (attempt 1 is the first try; ≥2 are retries).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt: Option<u32>,
}

/// A lifecycle transition tagged with the bundle/signature it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LifecycleRecord {
    pub id: String,
    pub event: LifecycleEvent,
}

/// A failure classification tagged with the bundle/signature it belongs to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FailureRecord {
    pub id: String,
    pub classification: FailureClassification,
}

/// The single telemetry envelope. `kind` tags the variant on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TelemetryEvent {
    Slot(SlotUpdate),
    Bundle(BundleEvent),
    Lifecycle(LifecycleRecord),
    Failure(FailureRecord),
    Health(HealthSnapshot),
    Decision(Decision),
}

impl TelemetryEvent {
    /// The NATS subject this event publishes to.
    pub fn subject(&self) -> &'static str {
        match self {
            TelemetryEvent::Slot(_) => "telemetry.slot",
            TelemetryEvent::Bundle(_) => "telemetry.bundle",
            TelemetryEvent::Lifecycle(_) => "telemetry.lifecycle",
            TelemetryEvent::Failure(_) => "telemetry.failure",
            TelemetryEvent::Health(_) => "telemetry.health",
            TelemetryEvent::Decision(d) => match d.decision_type {
                DecisionType::Tip => "decision.tip",
                DecisionType::Timing => "decision.timing",
                DecisionType::Retry => "decision.retry",
            },
        }
    }
}

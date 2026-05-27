//! Slot + commitment domain types.
//!
//! These mirror the Yellowstone gRPC (Dragon's Mouth) `SlotStatus` and `CommitmentLevel`
//! semantics so the ingestion layer maps stream messages onto our own stable contract rather
//! than leaking the proto types upward.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A Solana slot number. Newtype-free alias on purpose: slots are compared/ordered arithmetically
/// throughout the engine, and a transparent alias keeps that ergonomic while still being a named
/// type in signatures and schemas.
pub type Slot = u64;

/// Slot lifecycle status as surfaced by the Yellowstone slots subscription.
///
/// Ordering of the variants follows the real progression of a healthy slot:
/// `FirstShredReceived → CreatedBank → Completed → Processed → Confirmed → Finalized`, with
/// `Dead` as the off-ramp for a slot whose bank was abandoned (forked out).
///
/// The first three ("inter-slot") statuses are gated behind the provider's `interslot_updates`
/// support; the engine must degrade gracefully to the commitment statuses when they are absent
/// (flagged for experimental verification against the configured provider).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SlotStatus {
    /// Leader started producing the block; earliest possible signal. (inter-slot)
    FirstShredReceived,
    /// Validator created a bank for the slot and began replay. (inter-slot)
    CreatedBank,
    /// Block fully received/replayed and the bank frozen. (inter-slot)
    Completed,
    /// `processed` commitment: in a block, no supermajority vote yet, fork-revertible.
    Processed,
    /// `confirmed` commitment: supermajority (optimistic) vote observed.
    Confirmed,
    /// `finalized` commitment: rooted, irreversible (32-deep lockout).
    Finalized,
    /// The bank for this slot was marked dead (abandoned / forked out).
    Dead,
}

impl SlotStatus {
    /// Whether this is one of the finer-grained inter-slot statuses (vs a commitment status).
    pub fn is_interslot(self) -> bool {
        matches!(
            self,
            SlotStatus::FirstShredReceived | SlotStatus::CreatedBank | SlotStatus::Completed
        )
    }

    /// Monotonic rank used to decide whether an update *advances* a slot's status. Higher means
    /// later in the lifecycle. `Dead` is ranked highest because it is terminal.
    pub fn rank(self) -> u8 {
        match self {
            SlotStatus::FirstShredReceived => 0,
            SlotStatus::CreatedBank => 1,
            SlotStatus::Completed => 2,
            SlotStatus::Processed => 3,
            SlotStatus::Confirmed => 4,
            SlotStatus::Finalized => 5,
            SlotStatus::Dead => 6,
        }
    }
}

/// Subscription commitment level. Set request-globally on the Yellowstone subscription; we
/// subscribe low and track progression client-side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Commitment {
    Processed,
    Confirmed,
    Finalized,
}

/// A normalized slot update emitted by the ingestion layer (decoupled from the raw proto).
///
/// `parent` is `None` only when the stream did not provide it; when present it references a
/// *produced* block (skipped slots are never parents), which is what lets us detect ingestion
/// gaps by checking whether we ever saw the referenced parent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SlotUpdate {
    pub slot: Slot,
    pub parent: Option<Slot>,
    pub status: SlotStatus,
    pub ts: DateTime<Utc>,
}

impl SlotUpdate {
    /// Construct a slot update with an explicit timestamp.
    pub fn new(slot: Slot, parent: Option<Slot>, status: SlotStatus, ts: DateTime<Utc>) -> Self {
        Self {
            slot,
            parent,
            status,
            ts,
        }
    }
}

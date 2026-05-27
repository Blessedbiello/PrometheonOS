//! Slot-progression tracker: the pure, deterministic core of the ingestion layer.
//!
//! Responsibilities:
//! - Classify each incoming [`SlotUpdate`] into a [`SlotObservation`] (progressed / status
//!   advanced / leader-skipped / ingestion gap / dead / redundant).
//! - Maintain a reconnect **checkpoint** (highest slot seen at `Completed` or beyond).
//! - Plan `from_slot` replay on reconnect ([`ReconnectPlan`]).
//!
//! ## Skipped slots vs ingestion gaps — the load-bearing distinction
//!
//! A slot's `parent` always references a **produced** block (skipped slots are never parents).
//! Therefore:
//! - Slots strictly between `parent + 1` and `slot` were **skipped at the leader level** — normal,
//!   reported as `skipped` but not an error.
//! - If a slot's `parent` is itself a block we **never received an update for** (and it lies within
//!   our tracking window), we missed a produced block → that is an **ingestion gap**.
//!
//! State is bounded to the most recent [`SlotTracker::window`] slots; references older than the
//! window are treated as pre-history (no gap claim), since we cannot prove we missed them.

use std::collections::BTreeMap;

use prometheon_types::{Slot, SlotStatus, SlotUpdate};

/// Default number of recent slots retained for gap detection / status tracking.
/// ~4096 slots ≈ 27 min at 400 ms/slot — comfortably larger than any reconnect replay window.
pub const DEFAULT_WINDOW: u64 = 4096;

/// The classification of a single observed slot update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotObservation {
    /// First time we see this slot, and it extended the frontier. `skipped` lists the
    /// leader-skipped slots between the parent and this slot (normal, not an error).
    Progressed { slot: Slot, skipped: Vec<Slot> },
    /// A later-ranked status arrived for a slot we already track (e.g. `Processed` → `Confirmed`).
    StatusAdvanced {
        slot: Slot,
        from: SlotStatus,
        to: SlotStatus,
    },
    /// Ingestion gap: `slot` references a `parent` (a produced block) we never received, and the
    /// parent lies within our tracking window. We appear to have missed stream data.
    Gap { slot: Slot, parent: Slot },
    /// The bank for this slot was abandoned / forked out (`SlotStatus::Dead`).
    Dead { slot: Slot },
    /// A redundant or out-of-order update we can ignore (slot already at ≥ this status rank).
    Redundant { slot: Slot, status: SlotStatus },
}

/// How to resubscribe after a disconnect, given the server's earliest replayable slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconnectPlan {
    /// No checkpoint yet (or no replay info): (re)subscribe live with no replay.
    FromTip,
    /// Replay buffered updates starting at `from_slot` (= checkpoint + 1).
    Replay { from_slot: Slot },
    /// Our checkpoint predates the server's replay buffer: replaying is impossible, so we will
    /// miss `[wanted_from, earliest_available)`. Caller resumes live from `earliest_available`
    /// and reconciles the gap via RPC cross-check.
    GapBeyondBuffer {
        wanted_from: Slot,
        earliest_available: Slot,
    },
}

impl ReconnectPlan {
    /// The `from_slot` value to put on the reconnect `SubscribeRequest`.
    ///
    /// Only [`ReconnectPlan::Replay`] requests replay; the other variants resume live (no
    /// `from_slot`) — `FromTip` because there is nothing to replay, and `GapBeyondBuffer` because
    /// the desired slot predates the server buffer (the gap is reconciled via RPC cross-check).
    pub fn from_slot(&self) -> Option<Slot> {
        match self {
            ReconnectPlan::FromTip => None,
            ReconnectPlan::Replay { from_slot } => Some(*from_slot),
            ReconnectPlan::GapBeyondBuffer { .. } => None,
        }
    }
}

/// Tracks slot progression and reconnection state. Cheap to construct; `Default` uses
/// [`DEFAULT_WINDOW`].
#[derive(Debug, Clone)]
pub struct SlotTracker {
    window: u64,
    /// Highest status rank observed per slot (bounded to the window). Used for advance/redundant
    /// classification and for "have we seen this slot?" gap checks.
    status_by_slot: BTreeMap<Slot, SlotStatus>,
    /// Highest slot number seen at any status.
    highest: Option<Slot>,
    /// First slot we ever tracked; references below this are pre-history (never a gap).
    start: Option<Slot>,
    /// Highest slot seen at `Completed` or beyond — the reconnect checkpoint.
    checkpoint: Option<Slot>,
}

impl Default for SlotTracker {
    fn default() -> Self {
        Self::with_window(DEFAULT_WINDOW)
    }
}

impl SlotTracker {
    /// Construct a tracker retaining `window` recent slots for gap detection.
    pub fn with_window(window: u64) -> Self {
        Self {
            window: window.max(1),
            status_by_slot: BTreeMap::new(),
            highest: None,
            start: None,
            checkpoint: None,
        }
    }

    /// The reconnect checkpoint: highest slot seen at `Completed` or beyond, or `None`.
    pub fn checkpoint(&self) -> Option<Slot> {
        self.checkpoint
    }

    /// Highest slot number observed at any status.
    pub fn highest(&self) -> Option<Slot> {
        self.highest
    }

    /// Classify `update`, mutating internal state. See [`SlotObservation`].
    pub fn observe(&mut self, update: &SlotUpdate) -> SlotObservation {
        let SlotUpdate {
            slot,
            parent,
            status,
            ..
        } = *update;

        // Dead is terminal and never advances the checkpoint; record and report.
        if status == SlotStatus::Dead {
            self.record_status(slot, status);
            self.note_slot(slot);
            return SlotObservation::Dead { slot };
        }

        // Already tracking this slot? Decide advance vs redundant by status rank.
        if let Some(&prev) = self.status_by_slot.get(&slot) {
            if status.rank() > prev.rank() {
                self.record_status(slot, status);
                self.maybe_checkpoint(slot, status);
                return SlotObservation::StatusAdvanced {
                    slot,
                    from: prev,
                    to: status,
                };
            }
            return SlotObservation::Redundant { slot, status };
        }

        // First sighting of this slot. Establish the window start on the very first observation.
        if self.start.is_none() {
            self.start = Some(slot.min(parent.unwrap_or(slot)));
        }

        // Gap check: parent is a produced block; if it's within our window and we never saw it,
        // we missed data. (We must have already tracked at least one slot for this to apply.)
        if let Some(p) = parent {
            if self.is_within_window(p)
                && !self.status_by_slot.contains_key(&p)
                && self.has_history()
            {
                // Record the slot anyway so subsequent updates classify correctly.
                self.record_status(slot, status);
                self.note_slot(slot);
                self.maybe_checkpoint(slot, status);
                return SlotObservation::Gap { slot, parent: p };
            }
        }

        // Normal progression: report leader-skipped slots between parent+1 and slot.
        let skipped = self.skipped_between(parent, slot);
        self.record_status(slot, status);
        self.note_slot(slot);
        self.maybe_checkpoint(slot, status);
        SlotObservation::Progressed { slot, skipped }
    }

    /// Plan a reconnect given the server's earliest replayable slot (`SubscribeReplayInfo`).
    pub fn reconnect_plan(&self, earliest_available: Option<Slot>) -> ReconnectPlan {
        let Some(checkpoint) = self.checkpoint else {
            return ReconnectPlan::FromTip;
        };
        let wanted_from = checkpoint + 1;
        match earliest_available {
            None => ReconnectPlan::FromTip,
            Some(first) if wanted_from >= first => ReconnectPlan::Replay {
                from_slot: wanted_from,
            },
            Some(first) => ReconnectPlan::GapBeyondBuffer {
                wanted_from,
                earliest_available: first,
            },
        }
    }

    // ── internals ────────────────────────────────────────────────────────────────────────────

    /// Whether we have tracked at least one slot (gap detection is meaningless before that).
    fn has_history(&self) -> bool {
        self.highest.is_some()
    }

    /// Whether `slot` is recent enough to reason about (not pruned / not pre-window).
    fn is_within_window(&self, slot: Slot) -> bool {
        match (self.start, self.highest) {
            (Some(start), _) if slot < start => false,
            (_, Some(highest)) => slot + self.window >= highest,
            _ => true,
        }
    }

    /// Leader-skipped slots in `(parent, slot)` exclusive, when parent is known and within window.
    fn skipped_between(&self, parent: Option<Slot>, slot: Slot) -> Vec<Slot> {
        match parent {
            Some(p) if p + 1 < slot && self.is_within_window(p) => (p + 1..slot).collect(),
            _ => vec![],
        }
    }

    fn record_status(&mut self, slot: Slot, status: SlotStatus) {
        let entry = self.status_by_slot.entry(slot).or_insert(status);
        if status.rank() > entry.rank() {
            *entry = status;
        }
    }

    fn note_slot(&mut self, slot: Slot) {
        self.highest = Some(self.highest.map_or(slot, |h| h.max(slot)));
        self.prune();
    }

    fn maybe_checkpoint(&mut self, slot: Slot, status: SlotStatus) {
        if status.rank() >= SlotStatus::Completed.rank() && status != SlotStatus::Dead {
            self.checkpoint = Some(self.checkpoint.map_or(slot, |c| c.max(slot)));
        }
    }

    /// Drop tracked slots older than the window relative to the highest slot seen.
    fn prune(&mut self) {
        if let Some(highest) = self.highest {
            let cutoff = highest.saturating_sub(self.window);
            // Retain slots strictly newer than the cutoff.
            self.status_by_slot = self.status_by_slot.split_off(&(cutoff + 1));
        }
    }
}

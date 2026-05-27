//! Behavioural spec for the slot-progression tracker (Phase 1, written test-first).
//!
//! Covers: normal progression, leader-skipped slots (normal), per-slot status advancement,
//! redundant/out-of-order updates, ingestion-gap detection via the `parent` chain, dead slots,
//! reconnect-checkpoint computation, and `from_slot` replay planning.

use chrono::{TimeZone, Utc};
use prometheon_ingest::slot_tracker::{ReconnectPlan, SlotObservation, SlotTracker};
use prometheon_types::{SlotStatus, SlotUpdate};

/// Helper: build a SlotUpdate with a deterministic timestamp derived from the slot.
fn upd(slot: u64, parent: Option<u64>, status: SlotStatus) -> SlotUpdate {
    let ts = Utc.timestamp_opt(1_700_000_000 + slot as i64, 0).unwrap();
    SlotUpdate::new(slot, parent, status, ts)
}

#[test]
fn first_slot_progresses_and_sets_window_start() {
    let mut t = SlotTracker::default();
    let obs = t.observe(&upd(100, Some(99), SlotStatus::Completed));
    assert_eq!(
        obs,
        SlotObservation::Progressed {
            slot: 100,
            skipped: vec![]
        },
        "the very first slot cannot be a gap (parent is pre-window)"
    );
}

#[test]
fn contiguous_slots_progress_without_skips() {
    let mut t = SlotTracker::default();
    t.observe(&upd(100, Some(99), SlotStatus::Completed));
    let obs = t.observe(&upd(101, Some(100), SlotStatus::Completed));
    assert_eq!(
        obs,
        SlotObservation::Progressed {
            slot: 101,
            skipped: vec![]
        }
    );
}

#[test]
fn leader_skipped_slots_are_reported_but_not_a_gap() {
    let mut t = SlotTracker::default();
    t.observe(&upd(100, Some(99), SlotStatus::Completed));
    // 105's parent is 102 (a produced block we've... not seen). But the slots strictly between
    // parent+1 and slot are leader-skipped and normal. Here parent 102 is unseen → that's a gap,
    // so use a parent we HAVE seen to isolate the skip case:
    t.observe(&upd(102, Some(100), SlotStatus::Completed)); // 101 skipped at leader level
    let obs = t.observe(&upd(105, Some(102), SlotStatus::Completed)); // 103,104 skipped
    assert_eq!(
        obs,
        SlotObservation::Progressed {
            slot: 105,
            skipped: vec![103, 104]
        }
    );
}

#[test]
fn status_advances_for_known_slot() {
    let mut t = SlotTracker::default();
    t.observe(&upd(100, Some(99), SlotStatus::Processed));
    let obs = t.observe(&upd(100, Some(99), SlotStatus::Confirmed));
    assert_eq!(
        obs,
        SlotObservation::StatusAdvanced {
            slot: 100,
            from: SlotStatus::Processed,
            to: SlotStatus::Confirmed
        }
    );
}

#[test]
fn redundant_or_out_of_order_status_is_ignored() {
    let mut t = SlotTracker::default();
    t.observe(&upd(100, Some(99), SlotStatus::Confirmed));
    // A lower-ranked status arriving later is redundant (we already have Confirmed).
    let obs = t.observe(&upd(100, Some(99), SlotStatus::Processed));
    assert_eq!(
        obs,
        SlotObservation::Redundant {
            slot: 100,
            status: SlotStatus::Processed
        }
    );
}

#[test]
fn ingestion_gap_detected_when_parent_was_never_seen() {
    let mut t = SlotTracker::default();
    t.observe(&upd(50, Some(49), SlotStatus::Completed)); // window start = 50
                                                          // 53's parent is 52, a produced block (parents are always produced) we never received.
    let obs = t.observe(&upd(53, Some(52), SlotStatus::Completed));
    assert_eq!(
        obs,
        SlotObservation::Gap {
            slot: 53,
            parent: 52
        }
    );
}

#[test]
fn dead_slot_is_reported() {
    let mut t = SlotTracker::default();
    t.observe(&upd(100, Some(99), SlotStatus::Processed));
    let obs = t.observe(&upd(100, None, SlotStatus::Dead));
    assert_eq!(obs, SlotObservation::Dead { slot: 100 });
}

#[test]
fn checkpoint_tracks_highest_completed_or_beyond() {
    let mut t = SlotTracker::default();
    assert_eq!(
        t.checkpoint(),
        None,
        "no checkpoint before any completed slot"
    );
    t.observe(&upd(100, Some(99), SlotStatus::FirstShredReceived));
    assert_eq!(
        t.checkpoint(),
        None,
        "inter-slot statuses below Completed do not checkpoint"
    );
    t.observe(&upd(100, Some(99), SlotStatus::Completed));
    assert_eq!(t.checkpoint(), Some(100));
    t.observe(&upd(101, Some(100), SlotStatus::Confirmed));
    assert_eq!(t.checkpoint(), Some(101));
}

#[test]
fn reconnect_plan_without_checkpoint_is_from_tip() {
    let t = SlotTracker::default();
    assert_eq!(t.reconnect_plan(Some(1_000)), ReconnectPlan::FromTip);
    assert_eq!(t.reconnect_plan(None), ReconnectPlan::FromTip);
}

#[test]
fn reconnect_plan_replays_from_checkpoint_when_within_buffer() {
    let mut t = SlotTracker::default();
    t.observe(&upd(100, Some(99), SlotStatus::Completed));
    // server can replay from slot 50; our checkpoint is 100 → replay from 101.
    assert_eq!(
        t.reconnect_plan(Some(50)),
        ReconnectPlan::Replay { from_slot: 101 }
    );
}

#[test]
fn reconnect_plan_flags_gap_when_checkpoint_older_than_buffer() {
    let mut t = SlotTracker::default();
    t.observe(&upd(100, Some(99), SlotStatus::Completed));
    // server's earliest replayable slot is 200, but we want 101 → unavoidable gap.
    assert_eq!(
        t.reconnect_plan(Some(200)),
        ReconnectPlan::GapBeyondBuffer {
            wanted_from: 101,
            earliest_available: 200
        }
    );
}

#[test]
fn reconnect_plan_yields_from_slot_for_the_subscribe_request() {
    // Replay carries the from_slot; the two gap/tip variants resume live (None) and reconcile
    // any gap via RPC cross-check rather than requesting an unavailable replay.
    assert_eq!(ReconnectPlan::FromTip.from_slot(), None);
    assert_eq!(
        ReconnectPlan::Replay { from_slot: 101 }.from_slot(),
        Some(101)
    );
    assert_eq!(
        ReconnectPlan::GapBeyondBuffer {
            wanted_from: 101,
            earliest_available: 200
        }
        .from_slot(),
        None
    );
}

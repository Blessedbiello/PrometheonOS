//! Leader-window detection.
//!
//! Jito bundles only land when a **Jito-Solana validator** is the leader, so the engine times
//! submission for the upcoming Jito leader slots. [`crate::leader`] turns the Block Engine's
//! `getNextScheduledLeader` reading ([`NextLeader`]) into a simple go/hold signal; the math is pure
//! and unit-tested, and the AI agent's submission-timing decision can layer reasoning on top.

use prometheon_bundle::NextLeader;

/// How the current slot relates to the next Jito leader slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaderWindow {
    pub current_slot: u64,
    pub next_leader_slot: u64,
}

impl LeaderWindow {
    pub fn new(current_slot: u64, next_leader_slot: u64) -> Self {
        Self {
            current_slot,
            next_leader_slot,
        }
    }

    pub fn from_next(next: &NextLeader) -> Self {
        Self::new(next.current_slot, next.next_leader_slot)
    }

    /// Slots until the next Jito leader (0 if it's the current slot or already passed).
    pub fn slots_until(&self) -> u64 {
        self.next_leader_slot.saturating_sub(self.current_slot)
    }

    /// True if the next Jito leader is within `lookahead` slots (i.e. submit now to land soon).
    pub fn in_window(&self, lookahead: u64) -> bool {
        self.slots_until() <= lookahead
    }

    /// Submission gate: go when within `lookahead` of a Jito leader, hold otherwise. This is the
    /// deterministic default; the AI timing decision may widen/narrow it under elevated skip risk.
    pub fn should_submit_now(&self, lookahead: u64) -> bool {
        self.in_window(lookahead)
    }
}

/// The upcoming leader schedule from Solana RPC `getSlotLeaders`: `leaders[i]` is the validator
/// identity for `start_slot + i`.
///
/// This is the **reliable, no-auth** way to detect leader windows for submission timing. The Jito
/// searcher `getNextScheduledLeader` (which would also tell us *which* upcoming leaders run Jito) is
/// a gRPC searcher-API method needing approved auth — its HTTP form 404s — so we time against the
/// RPC schedule and rely on the Block Engine routing the bundle to the next Jito leader. The math is
/// pure and unit-tested; the AI submission-timing decision reasons over it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaderSchedule {
    pub start_slot: u64,
    pub leaders: Vec<String>,
}

impl LeaderSchedule {
    pub fn new(start_slot: u64, leaders: Vec<String>) -> Self {
        Self {
            start_slot,
            leaders,
        }
    }

    /// The leader producing `start_slot` (the current leader), if known.
    pub fn current_leader(&self) -> Option<&str> {
        self.leaders.first().map(|s| s.as_str())
    }

    /// Slots from `start_slot` until the leader identity changes (a fresh window begins). `None` if
    /// the whole observed schedule has a single leader. Submitting just after a change maximizes
    /// runway in the new window; right before one risks landing in the tail of a slot.
    pub fn slots_until_leader_change(&self) -> Option<u64> {
        let first = self.leaders.first()?;
        self.leaders
            .iter()
            .position(|l| l != first)
            .map(|i| i as u64)
    }

    /// The next slot at which the leader changes, if observed within the schedule.
    pub fn next_leader_change_slot(&self) -> Option<u64> {
        self.slots_until_leader_change()
            .map(|d| self.start_slot + d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn next(current: u64, leader: u64) -> NextLeader {
        NextLeader {
            current_slot: current,
            next_leader_slot: leader,
            next_leader_identity: None,
            next_leader_region: None,
        }
    }

    #[test]
    fn slots_until_and_window() {
        let w = LeaderWindow::from_next(&next(425_350_000, 425_350_003));
        assert_eq!(w.slots_until(), 3);
        assert!(w.in_window(4));
        assert!(w.should_submit_now(3));
        assert!(!w.in_window(2)); // hold: leader is 3 slots out, only willing to wait 2
    }

    #[test]
    fn current_slot_is_the_leader_means_submit_now() {
        let w = LeaderWindow::new(500, 500);
        assert_eq!(w.slots_until(), 0);
        assert!(w.should_submit_now(0));
    }

    #[test]
    fn passed_leader_saturates_to_zero() {
        // If the reported leader slot is behind us, slots_until saturates (don't underflow).
        let w = LeaderWindow::new(600, 500);
        assert_eq!(w.slots_until(), 0);
        assert!(w.in_window(0));
    }

    #[test]
    fn leader_schedule_finds_the_next_rotation() {
        // Same leader for 4 slots, then it changes at index 4.
        let s = LeaderSchedule::new(
            1_000,
            vec![
                "Val1".into(),
                "Val1".into(),
                "Val1".into(),
                "Val1".into(),
                "Val2".into(),
                "Val2".into(),
            ],
        );
        assert_eq!(s.current_leader(), Some("Val1"));
        assert_eq!(s.slots_until_leader_change(), Some(4));
        assert_eq!(s.next_leader_change_slot(), Some(1_004));
    }

    #[test]
    fn leader_schedule_with_one_leader_reports_no_change() {
        let s = LeaderSchedule::new(50, vec!["Solo".into(); 8]);
        assert_eq!(s.slots_until_leader_change(), None);
        assert_eq!(s.next_leader_change_slot(), None);
        let empty = LeaderSchedule::new(0, vec![]);
        assert_eq!(empty.current_leader(), None);
        assert_eq!(empty.slots_until_leader_change(), None);
    }
}

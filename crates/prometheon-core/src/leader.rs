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
}

//! Behavioural spec for the bounded ingest queue + drop accounting (Phase 1, test-first).
//!
//! Rationale: the gRPC receive loop must never block (blocking the stream makes us drift from the
//! chain tip and get force-disconnected). So when the worker pool falls behind we *drop* with an
//! explicit policy and account for it in telemetry — rather than apply back-pressure to the stream.

use prometheon_ingest::backpressure::{BoundedIngestQueue, DropPolicy, PushOutcome};

#[test]
fn pushes_under_capacity_are_admitted_in_fifo_order() {
    let mut q: BoundedIngestQueue<u32> = BoundedIngestQueue::new(3, DropPolicy::DropNewest);
    assert_eq!(q.push(1), PushOutcome::Admitted);
    assert_eq!(q.push(2), PushOutcome::Admitted);
    assert_eq!(q.depth(), 2);
    assert_eq!(q.pop(), Some(1));
    assert_eq!(q.pop(), Some(2));
    assert_eq!(q.pop(), None);
}

#[test]
fn drop_newest_preserves_queued_items_and_drops_the_incoming() {
    let mut q: BoundedIngestQueue<u32> = BoundedIngestQueue::new(2, DropPolicy::DropNewest);
    q.push(1);
    q.push(2);
    // Full: the new item is rejected, existing order preserved.
    assert_eq!(q.push(3), PushOutcome::DroppedNew);
    assert_eq!(q.depth(), 2);
    assert_eq!(q.pop(), Some(1));
    assert_eq!(q.pop(), Some(2));
}

#[test]
fn drop_oldest_evicts_head_and_admits_incoming() {
    let mut q: BoundedIngestQueue<u32> = BoundedIngestQueue::new(2, DropPolicy::DropOldest);
    q.push(1);
    q.push(2);
    // Full: evict the oldest (1) to admit the freshest (3).
    assert_eq!(q.push(3), PushOutcome::DroppedOldest(1));
    assert_eq!(q.depth(), 2);
    assert_eq!(q.pop(), Some(2));
    assert_eq!(q.pop(), Some(3));
}

#[test]
fn stats_account_for_received_dropped_and_high_water() {
    let mut q: BoundedIngestQueue<u32> = BoundedIngestQueue::new(2, DropPolicy::DropNewest);
    q.push(1);
    q.push(2);
    q.push(3); // dropped
    q.push(4); // dropped
    let s = q.stats();
    assert_eq!(s.received, 4);
    assert_eq!(s.dropped, 2);
    assert_eq!(s.admitted, 2);
    assert_eq!(s.depth, 2);
    assert_eq!(s.capacity, 2);
    assert_eq!(s.high_water, 2);
}

#[test]
fn high_water_tracks_peak_depth_even_after_draining() {
    let mut q: BoundedIngestQueue<u32> = BoundedIngestQueue::new(4, DropPolicy::DropOldest);
    q.push(1);
    q.push(2);
    q.push(3);
    assert_eq!(q.stats().high_water, 3);
    q.pop();
    q.pop();
    // Draining must not lower the recorded peak.
    assert_eq!(q.stats().high_water, 3);
    assert_eq!(q.depth(), 1);
}

#[test]
fn utilization_and_drop_rate_are_reported() {
    let mut q: BoundedIngestQueue<u32> = BoundedIngestQueue::new(4, DropPolicy::DropNewest);
    assert_eq!(q.utilization(), 0.0);
    q.push(1);
    q.push(2);
    assert_eq!(q.utilization(), 0.5);

    // 2 of 2 over-capacity pushes dropped → with 4 received total, drop_rate = 0.5.
    let mut q2: BoundedIngestQueue<u32> = BoundedIngestQueue::new(2, DropPolicy::DropNewest);
    q2.push(1);
    q2.push(2);
    q2.push(3);
    q2.push(4);
    assert_eq!(q2.drop_rate(), 0.5);
}

#[test]
fn drop_rate_is_zero_with_no_pushes() {
    let q: BoundedIngestQueue<u32> = BoundedIngestQueue::new(2, DropPolicy::DropNewest);
    assert_eq!(q.drop_rate(), 0.0);
}

#[test]
fn capacity_is_clamped_to_at_least_one() {
    let mut q: BoundedIngestQueue<u32> = BoundedIngestQueue::new(0, DropPolicy::DropNewest);
    assert_eq!(q.capacity(), 1);
    assert_eq!(q.push(1), PushOutcome::Admitted);
    assert_eq!(q.push(2), PushOutcome::DroppedNew);
}

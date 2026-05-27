//! Bounded ingest queue with explicit drop policy and accounting.
//!
//! ## Why this exists (and why not just a `tokio::mpsc` bounded channel)
//!
//! The Yellowstone receive loop must stay drained: per Triton's guidance, parsing + persisting
//! inside the receive loop fills the HTTP/2 window, backs data up server-side, and gets the client
//! **force-disconnected**. The standard fix is to push raw messages into a bounded queue and do the
//! heavy work in a separate worker pool.
//!
//! A `tokio::mpsc` bounded channel applies back-pressure by **blocking the sender** when full — but
//! our "sender" is the gRPC stream, which we cannot slow without drifting from the chain tip and
//! being disconnected. So when the workers fall behind we must **drop with an explicit policy** and
//! record it in telemetry, never block. This type is that policy + accounting core; the live client
//! wraps it (behind a lock / alongside a `tokio::mpsc`) and exports its stats to Prometheus.
//!
//! Pure and synchronous so the policy is fully unit-tested without async or network.

use std::collections::VecDeque;

/// What to do when a push arrives and the queue is at capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropPolicy {
    /// Reject the incoming item, preserving already-queued (older, in-order) items.
    /// Appropriate when in-order completeness matters more than freshness.
    DropNewest,
    /// Evict the oldest queued item to admit the incoming one.
    /// Appropriate when the freshest data matters most (e.g. latest slot status).
    DropOldest,
}

/// Outcome of a [`BoundedIngestQueue::push`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushOutcome<T> {
    /// The item was enqueued without dropping anything.
    Admitted,
    /// The queue was full and the incoming item was dropped (`DropNewest`).
    DroppedNew,
    /// The queue was full; the returned oldest item was evicted to admit the new one (`DropOldest`).
    DroppedOldest(T),
}

/// A snapshot of queue accounting for telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackpressureStats {
    /// Total push attempts.
    pub received: u64,
    /// Items that ended up enqueued (received − dropped).
    pub admitted: u64,
    /// Items dropped due to saturation (rejected newest, or evicted oldest).
    pub dropped: u64,
    /// Current number of queued items.
    pub depth: usize,
    /// Configured capacity.
    pub capacity: usize,
    /// Peak `depth` observed over the queue's lifetime.
    pub high_water: usize,
}

/// A bounded FIFO queue that drops (never blocks) under saturation and accounts for it.
#[derive(Debug)]
pub struct BoundedIngestQueue<T> {
    capacity: usize,
    policy: DropPolicy,
    buf: VecDeque<T>,
    received: u64,
    dropped: u64,
    high_water: usize,
}

impl<T> BoundedIngestQueue<T> {
    /// Create a queue with the given capacity (clamped to ≥ 1) and drop policy.
    pub fn new(capacity: usize, policy: DropPolicy) -> Self {
        let capacity = capacity.max(1);
        Self {
            capacity,
            policy,
            buf: VecDeque::with_capacity(capacity),
            received: 0,
            dropped: 0,
            high_water: 0,
        }
    }

    /// Offer an item. Never blocks; returns how the saturation policy resolved it.
    pub fn push(&mut self, item: T) -> PushOutcome<T> {
        self.received += 1;

        if self.buf.len() < self.capacity {
            self.buf.push_back(item);
            self.touch_high_water();
            return PushOutcome::Admitted;
        }

        // At capacity — apply the drop policy.
        match self.policy {
            DropPolicy::DropNewest => {
                self.dropped += 1;
                PushOutcome::DroppedNew
            }
            DropPolicy::DropOldest => {
                // Safe: at capacity ≥ 1 the buffer is non-empty.
                let evicted = self
                    .buf
                    .pop_front()
                    .expect("queue at capacity is non-empty");
                self.buf.push_back(item);
                self.dropped += 1;
                self.touch_high_water();
                PushOutcome::DroppedOldest(evicted)
            }
        }
    }

    /// Take the next item for a worker, FIFO. `None` when empty.
    pub fn pop(&mut self) -> Option<T> {
        self.buf.pop_front()
    }

    /// Current queued depth.
    pub fn depth(&self) -> usize {
        self.buf.len()
    }

    /// Configured capacity (post-clamp).
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Fraction of capacity currently used, `[0.0, 1.0]`. A backpressure signal for telemetry and
    /// the network-health model.
    pub fn utilization(&self) -> f64 {
        self.buf.len() as f64 / self.capacity as f64
    }

    /// Fraction of received items that were dropped, `[0.0, 1.0]`. `0.0` when nothing was received.
    pub fn drop_rate(&self) -> f64 {
        if self.received == 0 {
            0.0
        } else {
            self.dropped as f64 / self.received as f64
        }
    }

    /// A telemetry snapshot.
    pub fn stats(&self) -> BackpressureStats {
        BackpressureStats {
            received: self.received,
            admitted: self.received - self.dropped,
            dropped: self.dropped,
            depth: self.buf.len(),
            capacity: self.capacity,
            high_water: self.high_water,
        }
    }

    fn touch_high_water(&mut self) {
        if self.buf.len() > self.high_water {
            self.high_water = self.buf.len();
        }
    }
}

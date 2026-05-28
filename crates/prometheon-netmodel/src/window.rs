//! A bounded rolling window of `f64` samples with cheap summary statistics.
//!
//! Used by the network-health model to summarize recent latencies, slot times, and tip levels over
//! a sliding window. Population variance (not sample) is used — we treat the window as the
//! population of "recent behaviour", not a sample of a larger distribution.

use std::collections::VecDeque;

/// A fixed-capacity FIFO window; pushing past capacity evicts the oldest sample.
#[derive(Debug, Clone)]
pub struct RollingWindow {
    capacity: usize,
    samples: VecDeque<f64>,
}

impl RollingWindow {
    /// Create a window holding up to `capacity` (clamped to ≥ 1) samples.
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            capacity,
            samples: VecDeque::with_capacity(capacity),
        }
    }

    /// Add a sample, evicting the oldest if at capacity.
    pub fn push(&mut self, value: f64) {
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(value);
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// The most recent sample.
    pub fn latest(&self) -> Option<f64> {
        self.samples.back().copied()
    }

    /// Arithmetic mean, or `None` if empty.
    pub fn mean(&self) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }
        let sum: f64 = self.samples.iter().sum();
        Some(sum / self.samples.len() as f64)
    }

    /// Population variance, or `None` if empty. A single sample has variance 0.
    pub fn variance(&self) -> Option<f64> {
        let mean = self.mean()?;
        let n = self.samples.len() as f64;
        let sum_sq: f64 = self.samples.iter().map(|v| (v - mean).powi(2)).sum();
        Some(sum_sq / n)
    }
}

//! Retry orchestrator: per-saga attempt tracking + backoff scheduling.
//!
//! Wraps the [`crate::policy`] decision and [`crate::backoff`] math, tracking how many times each
//! bundle/saga (keyed by id) has failed and producing the next [`RetryAction`] plus the backoff
//! delay to wait before resubmitting.

use std::collections::HashMap;

use prometheon_failure::FailureClass;

use crate::backoff::backoff_ms;
use crate::policy::{decide_retry, RetryAction};

/// Tracks retry state across sagas.
#[derive(Debug, Clone)]
pub struct RetryOrchestrator {
    max_attempts: u32,
    base_ms: u64,
    max_ms: u64,
    /// Number of failed attempts seen per saga id.
    attempts: HashMap<String, u32>,
}

impl RetryOrchestrator {
    /// Create an orchestrator with an attempt cap and backoff bounds.
    pub fn new(max_attempts: u32, base_ms: u64, max_ms: u64) -> Self {
        Self {
            max_attempts,
            base_ms,
            max_ms,
            attempts: HashMap::new(),
        }
    }

    /// Record a failure for saga `id` with the classified `class`, returning the action and (when
    /// retrying) the backoff delay in milliseconds to wait before resubmitting.
    pub fn on_failure(&mut self, id: &str, class: FailureClass) -> (RetryAction, Option<u64>) {
        let attempt = {
            let counter = self.attempts.entry(id.to_string()).or_insert(0);
            *counter += 1;
            *counter
        };
        let action = decide_retry(class, attempt, self.max_attempts);
        let delay = match &action {
            // Backoff is keyed on the just-failed attempt: the 1st retry waits `base_ms`.
            RetryAction::Retry { .. } => Some(backoff_ms(attempt, self.base_ms, self.max_ms)),
            RetryAction::Abandon { .. } => None,
        };
        (action, delay)
    }

    /// Forget a saga's retry state (e.g. after it lands).
    pub fn clear(&mut self, id: &str) {
        self.attempts.remove(id);
    }

    /// How many failures have been recorded for a saga.
    pub fn attempts(&self, id: &str) -> u32 {
        self.attempts.get(id).copied().unwrap_or(0)
    }
}

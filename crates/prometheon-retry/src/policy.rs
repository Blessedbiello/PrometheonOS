//! Retry policy: decide whether and how to retry, from the failure class + attempt count.
//!
//! This is the deterministic policy the engine falls back to; the AI agent's retry decision
//! (Phase 5) can override it with reasoning. The policy is principled, not hardcoded: what changes
//! before a retry is derived from *why* it failed, and the tip is always recalculated from live
//! data (never a fixed value).

use prometheon_failure::FailureClass;
use serde::{Deserialize, Serialize};

/// The action to take after a failed attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetryAction {
    /// Stop retrying.
    Abandon { reason: String },
    /// Retry after changing the indicated parameters.
    Retry {
        /// Fetch a fresh blockhash before resubmitting.
        refresh_blockhash: bool,
        /// Recompute the tip from current live conditions before resubmitting.
        recalc_tip: bool,
        /// The attempt number this retry will be (1-indexed).
        next_attempt: u32,
    },
}

/// Decide the action for a failure of `class` after `attempt` (1-indexed, the attempt that just
/// failed), given `max_attempts`.
pub fn decide_retry(class: FailureClass, attempt: u32, max_attempts: u32) -> RetryAction {
    if !class.is_retryable() {
        return RetryAction::Abandon {
            reason: format!("{class:?} is not retryable"),
        };
    }
    if attempt >= max_attempts {
        return RetryAction::Abandon {
            reason: format!("attempt cap reached ({attempt}/{max_attempts})"),
        };
    }
    RetryAction::Retry {
        // Refresh the blockhash when the cause is the window itself (expiry or a timeout while the
        // window is closing); for other causes the blockhash is still valid.
        refresh_blockhash: matches!(
            class,
            FailureClass::ExpiredBlockhash | FailureClass::ConfirmationTimeout
        ),
        // Always re-price from live data on retry — conditions have changed since submission.
        recalc_tip: true,
        next_attempt: attempt + 1,
    }
}

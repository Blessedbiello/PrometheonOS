//! `prometheon-retry`
//!
//! Retry orchestration: a principled, AI-overridable policy that decides whether and how to retry
//! a failed submission (refresh blockhash, recalc tip) with exponential backoff and an attempt cap.
//! What changes before a retry is derived from the failure class — there is no hardcoded retry
//! flow. Built test-first; pure.

pub mod backoff;
pub mod orchestrator;
pub mod policy;

pub use backoff::{backoff_ms, with_jitter};
pub use orchestrator::RetryOrchestrator;
pub use policy::{decide_retry, RetryAction};

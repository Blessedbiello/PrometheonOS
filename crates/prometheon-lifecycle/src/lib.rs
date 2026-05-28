//! `prometheon-lifecycle`
//!
//! Transaction lifecycle tracking: a strict per-submission state machine over the commitment
//! stages (Submitted → Processed → Confirmed → Finalized, with failure branches), capturing slot
//! numbers, timestamps, and inter-stage latency deltas. Driven by the Yellowstone stream with RPC
//! cross-check. Built test-first.

pub mod lifecycle;

pub use lifecycle::{LifecycleEvent, LifecycleStage, TransactionLifecycle, TransitionOutcome};

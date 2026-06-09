//! `prometheon-core` — the orchestrator library + binaries.
//!
//! The library exposes the engine's wiring building blocks (config, the orchestration loop, sinks)
//! so the `prometheon` binary stays a thin entrypoint and the pieces remain unit-testable.

pub mod config;
pub mod engine;
pub mod rpc;
pub mod submission;
pub mod wallet;

pub use config::{Config, Network};
pub use submission::{next_saga_action, SagaAction, SubmissionOutcome};

//! `prometheon-core` — the orchestrator library + binaries.
//!
//! The library exposes the engine's wiring building blocks (config, the orchestration loop, sinks)
//! so the `prometheon` binary stays a thin entrypoint and the pieces remain unit-testable.

pub mod config;
pub mod engine;
pub mod leader;
pub mod metrics;
pub mod proof;
pub mod proof_run;
pub mod rpc;
pub mod saga;
pub mod sinks;
pub mod submission;
pub mod submit;
pub mod wallet;

pub use config::{Config, Network};
pub use saga::{run_saga, AttemptSpec, BaseBundle, DecisionSource, SagaConfig, Submitter};
pub use sinks::{EventSink, Sinks};
pub use submission::{next_saga_action, SagaAction, SubmissionOutcome};
pub use submit::{
    log_from_events, receipt_from_log, run_submit, Receipt, SignerSource, SubmitRequest,
    SubmitStrategy,
};

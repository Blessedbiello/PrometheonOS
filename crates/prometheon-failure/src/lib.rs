//! `prometheon-failure`
//!
//! Failure classification: maps observable signals to the failure taxonomy with a confidence score
//! and an evidence grade (Observable vs Inferred), mirroring `docs/FAILURE-TAXONOMY.md`. The output
//! feeds the AI agent's retry reasoning. Built test-first; pure (no network).

pub mod classify;

pub use classify::{
    classify, EvidenceGrade, FailureClass, FailureClassification, FailureSignals, OnChainError,
};

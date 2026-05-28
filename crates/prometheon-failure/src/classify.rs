//! Failure classifier.
//!
//! Maps a set of observable signals to a [`FailureClass`] with a confidence score and an evidence
//! grade. The ordering is deliberate (mirrors `docs/FAILURE-TAXONOMY.md`):
//! on-chain errors and block-height expiry are **Observable** and decided first/with high
//! confidence; fee-too-low and leader-miss are **Inferred** from patterns and score moderately.
//!
//! Pure and fully unit-tested — no network. The AI agent (Phase 5) consumes the classification +
//! confidence to reason about retries rather than re-deriving it.

use serde::{Deserialize, Serialize};

/// A decoded on-chain error (from `getBundleStatuses.err` / RPC).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnChainError {
    /// Transaction exceeded its compute-unit budget.
    ComputeBudgetExceeded,
    /// A program returned an instruction error.
    InstructionError,
    /// The signature was already processed (duplicate).
    DuplicateSignature,
    /// Any other on-chain error (carried for context).
    Other(String),
}

/// Whether a classification is directly observed or inferred from a pattern of signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceGrade {
    Observable,
    Inferred,
}

/// The taxonomy class of a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    ExpiredBlockhash,
    FeeTooLow,
    ComputeExceeded,
    BundleFailure,
    LeaderMiss,
    SkippedSlot,
    DuplicateSignature,
    InstructionError,
    ConfirmationTimeout,
    Unclassified,
}

impl FailureClass {
    /// Default retryability per the taxonomy. (The AI agent may override with reasoning.)
    pub fn is_retryable(self) -> bool {
        match self {
            FailureClass::ExpiredBlockhash
            | FailureClass::FeeTooLow
            | FailureClass::BundleFailure
            | FailureClass::LeaderMiss
            | FailureClass::SkippedSlot
            | FailureClass::ConfirmationTimeout => true,
            FailureClass::ComputeExceeded
            | FailureClass::DuplicateSignature
            | FailureClass::InstructionError
            | FailureClass::Unclassified => false,
        }
    }
}

/// Observable signals gathered about a failed/non-landed submission.
#[derive(Debug, Clone)]
pub struct FailureSignals {
    /// Decoded on-chain error, if the bundle/tx landed-and-failed.
    pub on_chain_error: Option<OnChainError>,
    /// Inflight status reported `Failed` across all regions.
    pub bundle_status_failed: bool,
    /// Current block height passed `lastValidBlockHeight`.
    pub block_height_exceeded: bool,
    /// `isBlockhashValid` result (if checked).
    pub blockhash_valid: Option<bool>,
    /// Whether the bundle landed anywhere.
    pub landed: bool,
    /// Tip we paid (lamports).
    pub tip_lamports: u64,
    /// Reference tip floor (median) at submission (lamports).
    pub tip_floor_p50_lamports: u64,
    /// The scheduled leader appears to have missed/produced no accepted block.
    pub leader_missed: bool,
    /// The target slot was skipped (observed on the slot stream).
    pub slot_skipped: bool,
    /// No status by the deadline while the blockhash was still valid.
    pub confirmation_timeout: bool,
}

/// A classification result with confidence, grade, retryability, and a human-readable rationale.
#[derive(Debug, Clone, PartialEq)]
pub struct FailureClassification {
    pub class: FailureClass,
    pub confidence: f64,
    pub grade: EvidenceGrade,
    pub retryable: bool,
    pub rationale: String,
}

impl FailureClassification {
    fn new(class: FailureClass, confidence: f64, grade: EvidenceGrade, rationale: &str) -> Self {
        Self {
            class,
            confidence,
            grade,
            retryable: class.is_retryable(),
            rationale: rationale.to_string(),
        }
    }
}

/// Classify a failure from its signals. Rules are applied in priority order: observable evidence
/// first, then inference.
pub fn classify(s: &FailureSignals) -> FailureClassification {
    use EvidenceGrade::*;
    use FailureClass as F;

    // 1. On-chain errors are directly observed and take precedence.
    if let Some(err) = &s.on_chain_error {
        return match err {
            OnChainError::ComputeBudgetExceeded => FailureClassification::new(
                F::ComputeExceeded,
                0.95,
                Observable,
                "on-chain ComputeBudgetExceeded: raise compute-unit limit or reduce work",
            ),
            OnChainError::DuplicateSignature => FailureClassification::new(
                F::DuplicateSignature,
                0.95,
                Observable,
                "duplicate signature already processed; rebuild with a fresh blockhash if intent is new",
            ),
            OnChainError::InstructionError => FailureClassification::new(
                F::InstructionError,
                0.9,
                Observable,
                "program returned an instruction error; decode and fix before resubmitting",
            ),
            OnChainError::Other(msg) => FailureClassification::new(
                F::BundleFailure,
                0.8,
                Observable,
                &format!("on-chain error: {msg}"),
            ),
        };
    }

    // 2. Bundle marked failed across all regions (no decoded on-chain error).
    if s.bundle_status_failed {
        return FailureClassification::new(
            F::BundleFailure,
            0.85,
            Observable,
            "inflight status Failed across all regions",
        );
    }

    // 3. Blockhash expiry is observable from block height / isBlockhashValid.
    if s.block_height_exceeded || s.blockhash_valid == Some(false) {
        return FailureClassification::new(
            F::ExpiredBlockhash,
            0.92,
            Observable,
            "block height passed lastValidBlockHeight / blockhash no longer valid",
        );
    }

    // Beyond here, the bundle did not land and the window may still be open.
    if !s.landed {
        // 4a. Skipped slot is observable on the slot stream.
        if s.slot_skipped {
            return FailureClassification::new(
                F::SkippedSlot,
                0.7,
                Observable,
                "target slot was skipped; retry into the next produced slot",
            );
        }
        // 4b. Leader miss is inferred (we cannot see the leader directly).
        if s.leader_missed {
            return FailureClassification::new(
                F::LeaderMiss,
                0.6,
                Inferred,
                "scheduled leader produced no accepted block; rebroadcast to next Jito leader",
            );
        }
        // 4c. Fee too low is inferred from tip below the floor while the window is open.
        if s.tip_floor_p50_lamports > 0 && s.tip_lamports < s.tip_floor_p50_lamports {
            let shortfall = (s.tip_floor_p50_lamports - s.tip_lamports) as f64
                / s.tip_floor_p50_lamports as f64; // (0, 1]
            let confidence = (0.55 + 0.30 * shortfall).clamp(0.55, 0.85);
            return FailureClassification::new(
                F::FeeTooLow,
                confidence,
                Inferred,
                "not landed while tip is below the median floor; raise tip/priority fee",
            );
        }
        // 4d. Confirmation timeout with the window still open.
        if s.confirmation_timeout {
            return FailureClassification::new(
                F::ConfirmationTimeout,
                0.7,
                Observable,
                "no status by deadline while blockhash valid; rebroadcast within the window",
            );
        }
    }

    // 5. Nothing matched.
    FailureClassification::new(
        F::Unclassified,
        0.3,
        Inferred,
        "no signal matched a known failure class; gather more telemetry",
    )
}

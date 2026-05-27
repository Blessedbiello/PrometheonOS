//! Jito bundle-status parsing and domain mapping.
//!
//! Robust landing detection (per Jito docs) uses two endpoints in sequence:
//! 1. [`parse_inflight_statuses`] — `getInflightBundleStatuses`, in-memory for ~5 minutes, gives a
//!    fast `Invalid | Pending | Failed | Landed` verdict (+ `landed_slot`).
//! 2. [`parse_bundle_statuses`] — `getBundleStatuses`, gives on-chain detail: contained signatures,
//!    landed `slot`, `confirmation_status` (processed/confirmed/finalized), and any `err`.
//!
//! Both responses share the `{ context: { slot }, value: [...] }` shape (the JSON-RPC `result`).

use serde::Deserialize;

/// Error parsing a status response.
#[derive(Debug, thiserror::Error)]
pub enum StatusError {
    #[error("status response was not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

// ── getInflightBundleStatuses ──────────────────────────────────────────────────────────────────

/// Inflight bundle status as reported by the Block Engine's in-memory view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum InflightStatus {
    /// No longer in the system (older than ~5 min, or never seen).
    Invalid,
    /// Not yet failed, landed, or invalid.
    Pending,
    /// Marked failed across all regions (not forwarded).
    Failed,
    /// Landed on-chain.
    Landed,
}

impl InflightStatus {
    /// Whether the bundle landed.
    pub fn is_landed(self) -> bool {
        matches!(self, InflightStatus::Landed)
    }
    /// Whether it is still in flight and worth polling.
    pub fn is_pending(self) -> bool {
        matches!(self, InflightStatus::Pending)
    }
    /// Whether it reached a terminal non-success state.
    pub fn is_terminal_failure(self) -> bool {
        matches!(self, InflightStatus::Failed | InflightStatus::Invalid)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct InflightEntry {
    pub bundle_id: String,
    pub status: InflightStatus,
    #[serde(default)]
    pub landed_slot: Option<u64>,
}

/// Parsed `getInflightBundleStatuses` result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InflightStatuses {
    pub context_slot: u64,
    pub value: Vec<InflightEntry>,
}

/// Parse a `getInflightBundleStatuses` result body.
pub fn parse_inflight_statuses(body: &str) -> Result<InflightStatuses, StatusError> {
    #[derive(Deserialize)]
    struct Ctx {
        slot: u64,
    }
    #[derive(Deserialize)]
    struct Raw {
        context: Ctx,
        value: Vec<InflightEntry>,
    }
    let raw: Raw = serde_json::from_str(body)?;
    Ok(InflightStatuses {
        context_slot: raw.context.slot,
        value: raw.value,
    })
}

// ── getBundleStatuses ──────────────────────────────────────────────────────────────────────────

/// On-chain confirmation level of a landed bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BundleConfirmation {
    Processed,
    Confirmed,
    Finalized,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BundleStatusEntry {
    pub bundle_id: String,
    #[serde(default)]
    pub transactions: Vec<String>,
    #[serde(default)]
    pub slot: u64,
    #[serde(default)]
    pub confirmation_status: Option<BundleConfirmation>,
    /// Raw error object (shape varies); presence means the bundle/tx carried an error.
    #[serde(default)]
    pub err: Option<serde_json::Value>,
}

impl BundleStatusEntry {
    /// Whether an error is present (`err` is non-null).
    pub fn has_error(&self) -> bool {
        self.err.as_ref().is_some_and(|v| !v.is_null())
    }
}

/// Parsed `getBundleStatuses` result.
#[derive(Debug, Clone, PartialEq)]
pub struct BundleStatuses {
    pub context_slot: u64,
    pub value: Vec<BundleStatusEntry>,
}

/// Parse a `getBundleStatuses` result body.
pub fn parse_bundle_statuses(body: &str) -> Result<BundleStatuses, StatusError> {
    #[derive(Deserialize)]
    struct Ctx {
        slot: u64,
    }
    #[derive(Deserialize)]
    struct Raw {
        context: Ctx,
        value: Vec<BundleStatusEntry>,
    }
    let raw: Raw = serde_json::from_str(body)?;
    Ok(BundleStatuses {
        context_slot: raw.context.slot,
        value: raw.value,
    })
}

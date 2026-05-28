//! The AI decision contract.
//!
//! This is the canonical, serializable shape of an AI strategist decision + its reasoning trace.
//! The TypeScript agent emits these (its zod schema mirrors this struct); the Rust core receives,
//! applies, persists, and forwards them. Keeping the contract here (the Rust side) makes it part of
//! the generated `contracts/` schema so both languages stay in sync.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which operational decision the agent made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DecisionType {
    /// How much to tip for a bundle.
    Tip,
    /// Whether to submit now or hold.
    Timing,
    /// Whether/how to retry a failed submission.
    Retry,
}

/// An AI decision with the full reasoning trace required for judge-visible "AI Demonstration".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Decision {
    pub decision_type: DecisionType,
    /// Short, human-readable action (e.g. `"tip 12000->18000 lamports"`).
    pub action: String,
    /// The model's reasoning for this decision.
    pub reasoning: String,
    /// Confidence in `[0,1]`.
    pub confidence: f64,
    /// Structured snapshot of the inputs the agent considered.
    pub inputs_considered: serde_json::Value,
    /// State before the decision (for before/after comparison), if applicable.
    pub before: Option<serde_json::Value>,
    /// State after the decision, if applicable.
    pub after: Option<serde_json::Value>,
    /// Which provider produced the decision (`anthropic` | `openai` | `ollama`).
    pub provider: String,
    /// How long the decision took (ms) — observability for the AI path.
    pub latency_ms: u64,
    pub ts: DateTime<Utc>,
}

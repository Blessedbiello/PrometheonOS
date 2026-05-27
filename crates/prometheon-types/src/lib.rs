//! `prometheon-types`
//!
//! Shared domain types and the cross-language schema source of truth. Types here derive
//! `serde` + `schemars` so JSON Schema (and downstream TypeScript types) can be generated from
//! them (wired in Phase 4). Keep this crate dependency-light and logic-free — it is the contract.

pub mod slot;

pub use slot::{Commitment, Slot, SlotStatus, SlotUpdate};

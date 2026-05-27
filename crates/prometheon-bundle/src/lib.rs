//! `prometheon-bundle`
//!
//! Jito bundle engine: live tip-floor data, dynamic tip sizing, tip-account selection, bundle
//! construction, `sendBundle`, and bundle-status tracking.
//!
//! Built bottom-up and test-first. The pure logic — tip computation, tip-floor parsing, status
//! parsing, tip-account selection — has no network or Solana dependency and is fully unit-tested.
//! Bundle assembly (Solana transactions) and the Block Engine HTTP client build on top.

pub mod assembly;
pub mod client;
pub mod fees;
pub mod jsonrpc;
pub mod status;
pub mod tip;
pub mod tip_accounts;
pub mod tip_floor;

pub use assembly::{build_tip_bundle_tx, self_transfer_ix, serialize_tx_base64, BundleParams};
pub use client::{BlockEngineClient, BlockEngineConfig, JitoError};
pub use fees::{priority_fee_lamports, total_fee_lamports, BASE_FEE_LAMPORTS_PER_SIG};

pub use status::{
    parse_bundle_statuses, parse_inflight_statuses, BundleConfirmation, BundleStatuses,
    InflightStatus, InflightStatuses,
};
pub use tip::{compute_tip, Percentile, TipDecision, TipStrategy};
pub use tip_accounts::{parse_tip_accounts, TipAccounts};
pub use tip_floor::{sol_to_lamports, TipFloor, TipFloorError};

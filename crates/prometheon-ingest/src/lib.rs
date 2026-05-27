//! `prometheon-ingest`
//!
//! Yellowstone gRPC ingestion layer: slot/leader/transaction subscriptions, reconnection with
//! `from_slot` replay, backpressure handling, and gap detection.
//!
//! Built bottom-up and test-first. The pure decision logic (slot progression, gap detection,
//! reconnect planning) lives in [`slot_tracker`] and is fully unit-tested without network; the
//! live gRPC client wiring builds on top of it.

pub mod backpressure;
pub mod slot_tracker;
pub mod status_map;
pub mod yellowstone;

pub use backpressure::{BackpressureStats, BoundedIngestQueue, DropPolicy, PushOutcome};
pub use slot_tracker::{ReconnectPlan, SlotObservation, SlotTracker};
pub use status_map::{commitment_to_code, slot_status_from_code};
pub use yellowstone::{
    build_subscribe_request, spawn, IngestCounters, IngestHandle, IngestMessage, SubscriptionSpec,
    TxStatus, YellowstoneConfig,
};

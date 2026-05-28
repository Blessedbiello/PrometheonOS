//! `prometheon-telemetry`
//!
//! The telemetry contract: a single typed [`events::TelemetryEvent`] envelope (with NATS subject
//! mapping) and the [`decision::Decision`] AI-reasoning trace. This is the wire format shared
//! across the Rust core, the TypeScript AI agent, and the dashboard.
//!
//! The transport sinks (NATS publisher, Postgres/TimescaleDB, Prometheus exporter) wrap this
//! contract and are wired during core integration — they require running services to exercise, so
//! the contract (pure, serde-tested) is built first to unblock the agent + dashboard.

pub mod decision;
pub mod events;

pub use decision::{Decision, DecisionType};
pub use events::{BundleEvent, BundlePhase, FailureRecord, LifecycleRecord, TelemetryEvent};

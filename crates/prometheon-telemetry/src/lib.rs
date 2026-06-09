//! `prometheon-telemetry`
//!
//! The telemetry contract: a single typed [`events::TelemetryEvent`] envelope (with NATS subject
//! mapping) and the [`decision::Decision`] AI-reasoning trace. This is the wire format shared
//! across the Rust core, the TypeScript AI agent, and the dashboard.
//!
//! The [`nats`] module is the first transport sink: a publisher for the event stream plus the AI
//! decision request/reply. The Postgres/TimescaleDB and Prometheus sinks follow. The wire shapes
//! are pure, serde-tested; live behaviour is exercised by env-gated integration tests.

pub mod decision;
pub mod events;
pub mod export;
pub mod nats;
pub mod postgres;

pub use decision::{Decision, DecisionType};
pub use events::{BundleEvent, BundlePhase, FailureRecord, LifecycleRecord, TelemetryEvent};
pub use nats::TelemetryBus;
pub use postgres::PostgresSink;

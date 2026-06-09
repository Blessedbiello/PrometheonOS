//! Schema generator — the single source of truth for the cross-language contract.
//!
//! The Rust telemetry types derive `schemars::JsonSchema`; this binary emits their JSON Schema to
//! `contracts/json-schema/`, from which the TypeScript types are generated (see
//! `scripts/gen-contracts.sh`). So one definition (the Rust struct) drives the Rust core, the
//! NATS/Postgres wire format, and the TS agent + dashboard.
//!
//! ```text
//! cargo run -p prometheon-telemetry --bin schema-gen            # write contracts/json-schema/*
//! cargo run -p prometheon-telemetry --bin schema-gen -- --check # CI: fail on drift
//! ```
//! Run from the workspace root (paths are relative to CWD).

use std::path::Path;

use prometheon_telemetry::{Decision, TelemetryEvent};
use schemars::schema_for;

fn main() -> anyhow::Result<()> {
    let check = std::env::args().any(|a| a == "--check");
    let out_dir = Path::new("contracts/json-schema");

    // TelemetryEvent is the envelope: its schema transitively defines every nested contract type
    // (SlotUpdate, BundleEvent, LifecycleRecord, FailureRecord, HealthSnapshot, Decision). Decision
    // is emitted standalone too since the AI agent produces it directly.
    let schemas = [
        (
            "telemetry-event.schema.json",
            serde_json::to_string_pretty(&schema_for!(TelemetryEvent))?,
        ),
        (
            "decision.schema.json",
            serde_json::to_string_pretty(&schema_for!(Decision))?,
        ),
    ];

    let mut drift = Vec::new();
    for (name, body) in &schemas {
        let path = out_dir.join(name);
        let content = format!("{body}\n");
        if check {
            let existing = std::fs::read_to_string(&path).unwrap_or_default();
            if existing != content {
                drift.push(name.to_string());
            }
        } else {
            std::fs::create_dir_all(out_dir)?;
            std::fs::write(&path, &content)?;
            println!("wrote {}", path.display());
        }
    }

    if check {
        if drift.is_empty() {
            println!("contracts/json-schema up to date");
        } else {
            anyhow::bail!(
                "contracts/ schema drift in: {} — run `cargo run -p prometheon-telemetry --bin schema-gen` and commit the result",
                drift.join(", ")
            );
        }
    }
    Ok(())
}

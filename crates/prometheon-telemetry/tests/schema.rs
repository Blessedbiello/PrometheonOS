//! The generated JSON Schema must transitively cover the whole telemetry contract — if a new field
//! or variant is added without a `JsonSchema` derive, this fails before CI's drift check does.

use prometheon_telemetry::{Decision, TelemetryEvent};
use schemars::schema_for;

#[test]
fn telemetry_event_schema_covers_every_nested_contract_type() {
    let json = serde_json::to_string(&schema_for!(TelemetryEvent)).unwrap();
    // Tag + one representative field from each variant's payload (inlined or via $defs).
    for needle in [
        "\"kind\"",         // internally-tagged envelope
        "slot",             // SlotUpdate
        "bundle_id",        // BundleEvent
        "stage",            // LifecycleEvent
        "congestion_score", // HealthSnapshot
        "decision_type",    // Decision
        "FailureClassification",
    ] {
        assert!(json.contains(needle), "schema missing `{needle}`");
    }
}

#[test]
fn decision_schema_has_the_reasoning_trace_fields() {
    let json = serde_json::to_string(&schema_for!(Decision)).unwrap();
    for needle in [
        "action",
        "reasoning",
        "confidence",
        "inputs_considered",
        "provider",
        "latency_ms",
    ] {
        assert!(json.contains(needle), "decision schema missing `{needle}`");
    }
}

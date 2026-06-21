//! Lifecycle-log export — the bounty deliverable: ≥10 real bundles with slot numbers, commitment
//! progression, timestamps, tip amounts, and failure classification, in JSON + explorer-linked
//! markdown.
//!
//! Assembled from the persisted `telemetry_event` rows (bundle / lifecycle / failure payloads). The
//! assembly + rendering are pure and unit-tested; the DB query is thin I/O behind [`export`].

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;

/// One recorded commitment transition for a bundle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StageRow {
    pub stage: String,
    pub slot: Option<i64>,
    pub ts: String,
    pub delta_ms: Option<i64>,
}

/// One bundle's full lifecycle, as exported.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LifecycleLogEntry {
    pub bundle_id: String,
    pub tip_lamports: u64,
    pub tip_account: String,
    pub region: String,
    pub signatures: Vec<String>,
    pub submitted_ts: String,
    pub stages: Vec<StageRow>,
    pub first_slot: Option<i64>,
    /// submit → confirmed latency (ms), summed from the recorded inter-stage deltas.
    pub confirmed_latency_ms: Option<i64>,
    pub final_stage: Option<String>,
    pub failure_class: Option<String>,
    pub failure_confidence: Option<f64>,
}

impl LifecycleLogEntry {
    fn new(bundle_id: String) -> Self {
        Self {
            bundle_id,
            tip_lamports: 0,
            tip_account: String::new(),
            region: String::new(),
            signatures: Vec::new(),
            submitted_ts: String::new(),
            stages: Vec::new(),
            first_slot: None,
            confirmed_latency_ms: None,
            final_stage: None,
            failure_class: None,
            failure_confidence: None,
        }
    }

    pub fn landed(&self) -> bool {
        self.stages
            .iter()
            .any(|s| matches!(s.stage.as_str(), "confirmed" | "finalized"))
    }
}

/// Assemble lifecycle-log entries from raw `bundle` / `lifecycle` / `failure` event payloads.
pub fn build_log(
    bundles: &[Value],
    lifecycles: &[Value],
    failures: &[Value],
) -> Vec<LifecycleLogEntry> {
    use std::collections::BTreeMap;
    let mut entries: BTreeMap<String, LifecycleLogEntry> = BTreeMap::new();

    for b in bundles {
        let id = b["bundle_id"].as_str().unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        let e = entries
            .entry(id.to_string())
            .or_insert_with(|| LifecycleLogEntry::new(id.to_string()));
        if e.tip_lamports == 0 {
            e.tip_lamports = b["tip_lamports"].as_u64().unwrap_or(0);
        }
        if e.tip_account.is_empty() {
            e.tip_account = b["tip_account"].as_str().unwrap_or_default().to_string();
        }
        if e.region.is_empty() {
            e.region = b["region"].as_str().unwrap_or_default().to_string();
        }
        if e.signatures.is_empty() {
            e.signatures = b["signatures"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
        }
        if e.submitted_ts.is_empty() {
            e.submitted_ts = b["ts"].as_str().unwrap_or_default().to_string();
        }
    }

    for l in lifecycles {
        let id = l["id"].as_str().unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        let ev = &l["event"];
        let e = entries
            .entry(id.to_string())
            .or_insert_with(|| LifecycleLogEntry::new(id.to_string()));
        e.stages.push(StageRow {
            stage: ev["stage"].as_str().unwrap_or_default().to_string(),
            slot: ev["slot"].as_i64(),
            ts: ev["ts"].as_str().unwrap_or_default().to_string(),
            delta_ms: ev["delta_ms_from_prev"].as_i64(),
        });
    }

    for f in failures {
        let id = f["id"].as_str().unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        let e = entries
            .entry(id.to_string())
            .or_insert_with(|| LifecycleLogEntry::new(id.to_string()));
        e.failure_class = f["classification"]["class"].as_str().map(String::from);
        e.failure_confidence = f["classification"]["confidence"].as_f64();
    }

    let mut out: Vec<LifecycleLogEntry> = entries.into_values().collect();
    for e in &mut out {
        e.stages.sort_by(|a, b| a.ts.cmp(&b.ts));
        e.first_slot = e.stages.iter().find_map(|s| s.slot);
        e.final_stage = e.stages.last().map(|s| s.stage.clone());
        // submit→confirmed: sum inter-stage deltas up to and including the confirmed stage.
        let mut acc = 0i64;
        let mut found = false;
        for s in &e.stages {
            acc += s.delta_ms.unwrap_or(0);
            if s.stage == "confirmed" {
                found = true;
                break;
            }
        }
        e.confirmed_latency_ms = found.then_some(acc);
    }
    out
}

fn explorer_block(base: &str, slot: i64) -> String {
    format!("[{slot}]({base}/block/{slot})")
}

/// Render the lifecycle log as submission-ready markdown with explorer links.
pub fn render_markdown(entries: &[LifecycleLogEntry], explorer_base: &str) -> String {
    let landed = entries.iter().filter(|e| e.landed()).count();
    let failed = entries.len() - landed;

    let mut out = String::new();
    out.push_str("# PrometheonOS — Bundle Lifecycle Log\n\n");
    out.push_str(&format!(
        "{} bundles · {landed} landed · {failed} failed. Slot numbers are verifiable on the Solana explorer.\n\n",
        entries.len()
    ));
    out.push_str(
        "| # | Bundle | Tip (lamports) | First slot | Progression | Submit→Confirmed | Failure |\n",
    );
    out.push_str(
        "|---|--------|----------------|-----------|-------------|------------------|---------|\n",
    );
    for (i, e) in entries.iter().enumerate() {
        let bundle = if e.bundle_id.len() > 12 {
            format!(
                "{}…{}",
                &e.bundle_id[..6],
                &e.bundle_id[e.bundle_id.len() - 4..]
            )
        } else {
            e.bundle_id.clone()
        };
        let slot = e
            .first_slot
            .map(|s| explorer_block(explorer_base, s))
            .unwrap_or_else(|| "—".into());
        let progression = e
            .stages
            .iter()
            .map(|s| s.stage.as_str())
            .collect::<Vec<_>>()
            .join("→");
        let latency = e
            .confirmed_latency_ms
            .map(|ms| format!("{ms} ms"))
            .unwrap_or_else(|| "—".into());
        let failure = e.failure_class.clone().unwrap_or_else(|| "—".into());
        out.push_str(&format!(
            "| {} | `{}` | {} | {} | {} | {} | {} |\n",
            i + 1,
            bundle,
            e.tip_lamports,
            slot,
            if progression.is_empty() {
                "—".into()
            } else {
                progression
            },
            latency,
            failure,
        ));
    }
    out
}

/// Render the lifecycle log as pretty JSON.
pub fn render_json(entries: &[LifecycleLogEntry]) -> anyhow::Result<String> {
    Ok(serde_json::to_string_pretty(entries)?)
}

/// Render the **AI Decision Timeline** — the chronological reasoning traces the agent produced during
/// the run (tip / timing / retry). This is the judge-visible evidence for the "AI Demonstration"
/// axis: every decision shows its action, confidence, provider, and the reasoning behind it.
pub fn render_decisions_markdown(decisions: &[Value]) -> String {
    let mut out = String::new();
    out.push_str("\n## AI Decision Timeline\n\n");
    if decisions.is_empty() {
        out.push_str("_No AI decisions were recorded for this run._\n");
        return out;
    }
    out.push_str(&format!(
        "{} AI decisions recorded during the run (the agent owns the tip + autonomous-retry decisions).\n\n",
        decisions.len()
    ));
    for (i, d) in decisions.iter().enumerate() {
        let dtype = d["decision_type"].as_str().unwrap_or("?");
        let action = d["action"].as_str().unwrap_or("");
        let reasoning = d["reasoning"].as_str().unwrap_or("");
        let conf = d["confidence"].as_f64().unwrap_or(0.0);
        let provider = d["provider"].as_str().unwrap_or("");
        let ts = d["ts"].as_str().unwrap_or("");
        out.push_str(&format!(
            "**{}. [{dtype}]** {action} — confidence {conf:.2} · {provider} · {ts}\n\n",
            i + 1
        ));
        if !reasoning.is_empty() {
            out.push_str(&format!("> {reasoning}\n\n"));
        }
        let (before, after) = (&d["before"], &d["after"]);
        if !before.is_null() || !after.is_null() {
            out.push_str(&format!("`before: {before}  →  after: {after}`\n\n"));
        }
    }
    out
}

/// Query the persisted events and build the lifecycle log (most recent first by insertion).
pub async fn export(pool: &sqlx::PgPool) -> anyhow::Result<Vec<LifecycleLogEntry>> {
    let bundles = fetch_payloads(pool, "bundle").await?;
    let lifecycles = fetch_payloads(pool, "lifecycle").await?;
    let failures = fetch_payloads(pool, "failure").await?;
    Ok(build_log(&bundles, &lifecycles, &failures))
}

/// Fetch the persisted AI `decision` events (chronological) for the decision-timeline section.
pub async fn fetch_decisions(pool: &sqlx::PgPool) -> anyhow::Result<Vec<Value>> {
    fetch_payloads(pool, "decision").await
}

async fn fetch_payloads(pool: &sqlx::PgPool, kind: &str) -> anyhow::Result<Vec<Value>> {
    // Cast jsonb → text so we don't need sqlx's json feature; parse in Rust.
    let rows = sqlx::query(
        "SELECT payload::text AS p FROM telemetry_event WHERE kind = $1 ORDER BY recorded_at",
    )
    .bind(kind)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let s: String = row.try_get("p")?;
        if let Ok(v) = serde_json::from_str::<Value>(&s) {
            out.push(v);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixtures() -> (Vec<Value>, Vec<Value>, Vec<Value>) {
        let bundles = vec![
            json!({"kind":"bundle","bundle_id":"B1","tip_lamports":14500,"tip_account":"Tip1","region":"ny","signatures":["sigA"],"phase":"submitted","ts":"2026-06-09T00:00:00.000Z"}),
            json!({"kind":"bundle","bundle_id":"B2","tip_lamports":3000,"tip_account":"Tip2","region":"ny","signatures":["sigB"],"phase":"submitted","ts":"2026-06-09T00:01:00.000Z"}),
        ];
        let lifecycles = vec![
            json!({"kind":"lifecycle","id":"B1","event":{"stage":"submitted","slot":null,"ts":"2026-06-09T00:00:00.000Z","delta_ms_from_prev":null}}),
            json!({"kind":"lifecycle","id":"B1","event":{"stage":"processed","slot":425000100,"ts":"2026-06-09T00:00:00.500Z","delta_ms_from_prev":500}}),
            json!({"kind":"lifecycle","id":"B1","event":{"stage":"confirmed","slot":425000100,"ts":"2026-06-09T00:00:01.700Z","delta_ms_from_prev":1200}}),
            json!({"kind":"lifecycle","id":"B2","event":{"stage":"submitted","slot":null,"ts":"2026-06-09T00:01:00.000Z","delta_ms_from_prev":null}}),
            json!({"kind":"lifecycle","id":"B2","event":{"stage":"expired","slot":null,"ts":"2026-06-09T00:02:30.000Z","delta_ms_from_prev":90000}}),
        ];
        let failures = vec![
            json!({"kind":"failure","id":"B2","classification":{"class":"expired_blockhash","confidence":0.92}}),
        ];
        (bundles, lifecycles, failures)
    }

    #[test]
    fn builds_entries_with_progression_latency_and_failure() {
        let (b, l, f) = fixtures();
        let log = build_log(&b, &l, &f);
        assert_eq!(log.len(), 2);

        let b1 = log.iter().find(|e| e.bundle_id == "B1").unwrap();
        assert_eq!(b1.tip_lamports, 14500);
        assert_eq!(b1.first_slot, Some(425_000_100));
        assert_eq!(b1.final_stage.as_deref(), Some("confirmed"));
        assert_eq!(b1.confirmed_latency_ms, Some(1700)); // 500 + 1200
        assert!(b1.landed());
        assert!(b1.failure_class.is_none());

        let b2 = log.iter().find(|e| e.bundle_id == "B2").unwrap();
        assert!(!b2.landed());
        assert_eq!(b2.failure_class.as_deref(), Some("expired_blockhash"));
        assert_eq!(b2.final_stage.as_deref(), Some("expired"));
    }

    #[test]
    fn markdown_has_a_row_per_bundle_with_explorer_links() {
        let (b, l, f) = fixtures();
        let md = render_markdown(&build_log(&b, &l, &f), "https://explorer.solana.com");
        assert!(md.contains("2 bundles · 1 landed · 1 failed"));
        assert!(md.contains("https://explorer.solana.com/block/425000100"));
        assert!(md.contains("submitted→processed→confirmed"));
        assert!(md.contains("expired_blockhash"));
    }

    #[test]
    fn decision_timeline_renders_reasoning() {
        let decisions = vec![
            json!({"kind":"decision","decision_type":"tip","action":"tip 15000 lamports","reasoning":"floor + congestion","confidence":0.85,"provider":"anthropic","ts":"2026-06-21T00:00:00Z","before":null,"after":{"tip":15000}}),
            json!({"kind":"decision","decision_type":"retry","action":"refresh + re-price","reasoning":"blockhash expired; refresh and bump tip","confidence":0.9,"provider":"anthropic","ts":"2026-06-21T00:01:00Z","before":{"tip":15000},"after":{"refresh_blockhash":true,"tip":25000}}),
        ];
        let md = render_decisions_markdown(&decisions);
        assert!(md.contains("AI Decision Timeline"));
        assert!(md.contains("2 AI decisions"));
        assert!(md.contains("[retry]"));
        assert!(md.contains("blockhash expired; refresh and bump tip"));
        assert!(md.contains("refresh_blockhash"));
        // Empty case is honest.
        assert!(render_decisions_markdown(&[]).contains("No AI decisions"));
    }

    #[test]
    fn json_round_trips() {
        let (b, l, f) = fixtures();
        let log = build_log(&b, &l, &f);
        let s = render_json(&log).unwrap();
        let back: Vec<LifecycleLogEntry> = serde_json::from_str(&s).unwrap();
        assert_eq!(back, log);
    }
}

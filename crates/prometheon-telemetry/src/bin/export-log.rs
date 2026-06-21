//! Export the bundle lifecycle log from Postgres to `logs/lifecycle-log.{json,md}`.
//!
//! ```text
//! DATABASE_URL=postgres://… EXPLORER_BASE=https://explorer.solana.com \
//!   cargo run -p prometheon-telemetry --bin export-log
//! ```
//! Run after a (live) proof run has persisted bundle/lifecycle/failure events.

use std::path::Path;

use prometheon_telemetry::export::{
    export, fetch_decisions, render_decisions_markdown, render_json, render_markdown,
};
use prometheon_telemetry::PostgresSink;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let db = std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL not set"))?;
    let explorer = std::env::var("EXPLORER_BASE")
        .unwrap_or_else(|_| "https://explorer.solana.com".to_string());

    let sink = PostgresSink::connect(&db).await?;
    let entries = export(sink.pool()).await?;
    let decisions = fetch_decisions(sink.pool()).await?;

    let out_dir = Path::new("logs");
    std::fs::create_dir_all(out_dir)?;
    std::fs::write(out_dir.join("lifecycle-log.json"), render_json(&entries)?)?;
    // Markdown: the per-bundle lifecycle table + the AI Decision Timeline (judge-visible reasoning).
    let mut md = render_markdown(&entries, &explorer);
    md.push_str(&render_decisions_markdown(&decisions));
    std::fs::write(out_dir.join("lifecycle-log.md"), md)?;

    let landed = entries.iter().filter(|e| e.landed()).count();
    println!(
        "exported {} bundles ({} landed, {} failed) → logs/lifecycle-log.{{json,md}}",
        entries.len(),
        landed,
        entries.len() - landed
    );
    Ok(())
}

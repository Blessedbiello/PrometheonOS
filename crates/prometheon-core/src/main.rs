//! `prometheon-core` — orchestrator binary.
//!
//! Wires the engine crates, the NATS decision request/reply channel, and config.
//! Skeleton only; real wiring arrives in later phases.

fn main() {
    println!(
        "PrometheonOS — Autonomous Solana Execution Intelligence Engine (v{})",
        env!("CARGO_PKG_VERSION")
    );
    println!("scaffold: no engine wired yet. See TASKS.md for phase status.");
}

//! Live AI-in-the-loop saga — Rust core ↔ NATS ↔ the TypeScript agent.
//!
//! Proves the headline "AI Demonstration" axis end-to-end: the agent owns the tip decision on every
//! attempt, re-prices as congestion rises, and the deterministic saga drives autonomous retries
//! (refresh blockhash on expiry, re-price always) until a landing.
//!
//! Env-gated so the default `cargo test`/CI skips it. Run with the agent up on MockProvider:
//! ```text
//! # terminal 1
//! LLM_PROVIDER=mock NATS_URL=nats://localhost:4222 pnpm --filter @prometheon/ai-agent start
//! # terminal 2
//! AGENT_NATS_URL=nats://localhost:4222 cargo test -p prometheon-core --test saga_agent -- --nocapture
//! ```

use std::time::Duration;

use chrono::Utc;
use prometheon_core::submission::{next_saga_action, resolve_tip, tip_context, SubmissionOutcome};
use prometheon_core::SagaAction;
use prometheon_faultinject::{normal_signals, FaultScenario};
use prometheon_netmodel::HealthSnapshot;
use prometheon_telemetry::{DecisionType, TelemetryBus};

fn agent_url() -> Option<String> {
    std::env::var("AGENT_NATS_URL")
        .ok()
        .filter(|v| !v.is_empty())
}

fn snapshot_with_congestion(congestion: f64, tip_floor: u64) -> HealthSnapshot {
    HealthSnapshot {
        ts: Utc::now(),
        congestion_score: congestion,
        slot_stability_score: 1.0 - congestion,
        bundle_landing_probability: 0.8,
        retry_success_rate: 0.5,
        tip_efficiency_ratio: 0.0,
        cost_per_successful_landing: None,
        avg_confirmed_latency_ms: Some(700.0),
        confirm_latency_variance_ms: None,
        tip_floor_lamports: tip_floor,
    }
}

fn failed(scenario: FaultScenario) -> SubmissionOutcome {
    let mut signals = normal_signals();
    scenario.apply(&mut signals);
    SubmissionOutcome::Failed { signals }
}

#[tokio::test]
async fn ai_prices_tips_and_saga_retries_until_landing() {
    let Some(url) = agent_url() else {
        eprintln!("skipping: AGENT_NATS_URL not set (needs the agent running on MockProvider)");
        return;
    };

    let bus = TelemetryBus::connect(&url).await.expect("connect");
    const FLOOR: u64 = 10_000;
    const MAX: u32 = 3;

    // Each attempt: rising congestion + a scripted outcome. Attempts 1-2 fail (expiry, low tip),
    // attempt 3 lands. The AI decides the tip each time from the (escalating) congestion context.
    let plan = [
        (0.1, failed(FaultScenario::BlockhashExpiry)),
        (
            0.5,
            failed(FaultScenario::LowTip {
                tip_lamports: 1_000,
            }),
        ),
        (0.9, SubmissionOutcome::Landed { slot: 425_000_000 }),
    ];

    let mut tips = Vec::new();
    let mut actions = Vec::new();

    for (attempt, (congestion, outcome)) in plan.iter().enumerate() {
        let attempt = attempt as u32 + 1;
        let snap = snapshot_with_congestion(*congestion, FLOOR);

        // --- AI owns the tip decision for this attempt ---
        let decision = bus
            .request_decision(
                DecisionType::Tip,
                tip_context(&snap, attempt - 1, tips.last().copied()),
                Duration::from_secs(5),
            )
            .await
            .expect("agent tip decision");

        assert_eq!(decision.decision_type, DecisionType::Tip);
        assert_eq!(decision.provider, "mock");
        assert!(
            !decision.reasoning.is_empty(),
            "decision must carry reasoning"
        );

        let tip = resolve_tip(Some(&decision), FLOOR);
        eprintln!(
            "attempt {attempt}: congestion={congestion} -> tip={tip} lamports ({})",
            decision.action
        );
        tips.push(tip);

        // --- deterministic saga decides what to do next from the outcome ---
        actions.push(next_saga_action(outcome, attempt, MAX));
    }

    // The AI re-priced upward as congestion rose (MockProvider: floor * (1 + 0.5 * congestion)).
    assert_eq!(
        tips,
        vec![10_500, 12_500, 14_500],
        "tips must track congestion"
    );
    assert!(tips[2] > tips[0], "tip must rise with congestion");

    // Attempt 1 (expiry) -> retry WITH blockhash refresh; attempt 2 (low tip) -> retry, no refresh.
    assert_eq!(
        actions[0],
        SagaAction::Submit {
            next_attempt: 2,
            refresh_blockhash: true,
            recalc_tip: true,
        }
    );
    assert_eq!(
        actions[1],
        SagaAction::Submit {
            next_attempt: 3,
            refresh_blockhash: false,
            recalc_tip: true,
        }
    );
    assert_eq!(actions[2], SagaAction::Landed { slot: 425_000_000 });
}

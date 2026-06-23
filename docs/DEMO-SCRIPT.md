# PrometheonOS — Demo Video Shot List (~2.5 min)

The winning artifact is **one uncut sequence**: inject a blockhash-expiry → the AI reasons about it in
plain English → refreshes + re-prices → resubmits → the bundle **lands** → open that exact slot on a
Solana explorer. Record at 1080p+, terminal font large enough to read. Keep it under 3 minutes.

## Pre-flight (do before recording — don't film this)

```bash
# infra up: NATS + Postgres(+Timescale) + Prometheus
docker compose -f infra/docker-compose.yml up -d
cargo run -p prometheon-core --bin preflight        # expect all ✓ (RPC, tip floor, Yellowstone)
```
- `.env` filled (SolInfra RPC + Yellowstone, `JITO_*_MAINNET`, `WALLET_KEYPAIR_PATH_MAINNET`,
  `DATABASE_URL`, `ANTHROPIC_API_KEY`).
- Mainnet wallet **funded** (~$5–15 SOL). Browser open to `https://explorer.solana.com`.

## Layout while recording
Three panes visible at once if possible:
1. **Dashboard** — `http://localhost:3000` (full screen or large). Header badge should read **live**.
2. **AI agent log** — `LLM_PROVIDER=anthropic pnpm --filter @prometheon/ai-agent start`
3. **Proof run** terminal.

---

## Beat-by-beat

**0:00–0:20 — The thesis (voiceover over the live dashboard).**
"On Solana, sending a transaction is the easy part. PrometheonOS is an execution-intelligence engine:
it streams the network live, and an AI agent makes — and explains — the real operational decisions."
Show the dashboard: slots streaming, Network Health (congestion / slot stability), the (empty) AI
Decision Timeline. Point out the **live** badge (honest: it reads *simulated* when there's no bus).

**0:20–0:40 — Start the run.**
```bash
NETWORK=mainnet LLM_PROVIDER=anthropic ./scripts/run-proof.sh 12 low-tip:1,stale-blockhash:1
```
Narrate: "Twelve bundles. Ten clean, plus two deliberate failures — a sub-floor tip and an expired
blockhash — so you can watch the agent recover them." Show the proof printing the live tip floor →
congestion, and the **leader window** line (current leader + slots-to-rotation from `getSlotLeaders`).

**0:40–1:10 — Tip + timing decisions appear (cut to dashboard).**
As bundles submit, the **AI Decision Timeline** fills with `tip` decisions (and one `timing`
decision) — each with confidence, provider `anthropic`, and reasoning citing the floor + congestion.
Narrate: "Every tip is the agent's call from live data — never hardcoded — and the core clamps it to
policy bounds as a safety net."

**1:10–1:55 — The money shot: autonomous recovery.**
The stale-blockhash bundle doesn't land. Cut to the timeline as a **`retry` decision** appears — read
the reasoning aloud: *"blockhash expired, height past lastValidBlockHeight; refresh and bump the tip
as congestion rose."* Show the bundle row go **attempt 1 = expired_blockhash → attempt 2 = confirmed →
finalized**. Narrate: "The agent detected it, reasoned about the cause, refreshed the blockhash,
re-priced, and resubmitted — autonomously. No hardcoded retry flow."

**1:55–2:20 — Prove it's real (explorer).**
```bash
cat logs/lifecycle-log.md        # show the table + the AI Decision Timeline section
```
Copy a **first-slot** number from a landed row → paste into `explorer.solana.com` → show the block /
the transaction signature on-chain. Narrate: "These slots are verifiable — this ran on real mainnet
infrastructure."

**2:20–2:35 — Close.**
"Stream-confirmed lifecycle, dynamic tips, classified failures, and an AI that owns the retry decision
end-to-end — with the full reasoning trace persisted. That's PrometheonOS." Show the architecture
diagram (or the public arch-doc URL) for one beat.

---

## Capture checklist (for the written submission)
- [ ] The `retry` decision card (screenshot) — reasoning legible.
- [ ] The lifecycle-log row showing `attempt1 expired → attempt2 finalized`.
- [ ] The explorer page for one landed slot (URL in frame).
- [ ] `logs/lifecycle-log.md` (commit it / attach it).
- [ ] Dashboard with the **live** badge (not simulated) during the run.

## If something misbehaves on the day
- Agent down / no key → decisions show `provider: policy-fallback` (still a visible decision); the run
  still lands. Prefer `anthropic` for the demo so the reasoning is real model output.
- Stale-blockhash injection waits for the blockhash to actually expire (~60–90s) before submitting —
  that pause is expected; trim it in edit.
- Only one Yellowstone stream is allowed (SolInfra Ace): do **not** also run `prometheon` (the engine)
  during the proof — the proof owns the stream.

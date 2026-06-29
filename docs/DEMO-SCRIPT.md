# PrometheonOS — Demo Video Shot List (~2 min)

The winning artifact is **one legible sequence**: a transaction **fails on Solana mainnet**, an AI reads
the failure in plain English, pulls the right lever, and the bundle **recovers to a finalized landing you
can click on the explorer** — shown on the **Recovery Rail** dashboard. Record at 1080p+, large readable
type. Keep it under ~2.5 minutes; the first 12 seconds must land the whole story.

## Why this records reliably (read first)
The dashboard defaults to the **`proof-replay`** source — a *deterministic* replay of the committed
mainnet run (real on-chain data, real explorer links, the same 12-landed + 2-AI-recovered telemetry).
The hero plays **on cue, every time**, with no live run to babysit and no 60–90 s expiry waits. The
`?t=<ms>` URL param scrubs to an exact frame, so you can pre-park money shots. It is **honest**: the badge
reads `proof-replay`, never `live`. (A live run is optional flex — see the end — but the replay is the
demo.)

## Pre-flight (do before recording — don't film this)
```bash
pnpm --filter @prometheon/dashboard dev      # http://localhost:3000  (defaults to proof-replay)
```
- Browser tab 1: the dashboard. Tab 2: `https://explorer.solana.com` (ready to paste a slot).
- Maximize the window; hide bookmarks/devtools; dark OS theme so the instrument fills the frame.
- Optional: open these three URLs in advance to scrub instantly:
  - `http://localhost:3000/?t=34500` — **money shot**: both faults healed, explorer-linked slots.
  - `http://localhost:3000/?t=31000` — recovery **in progress** (operator amber, one row still healing).
  - `http://localhost:3000/` — the **live loop** (the rail animates the full run in ~34 s).

---

## Beat-by-beat

**0:00–0:12 — COLD OPEN (no narration, pure payoff).**
Open on `?t=34500`. Split-screen (or quick cut): LEFT = the rail's two recovery rows —
`b11  fee_too_low 80% → ↑ raise tip` and `b12  expired_blockhash 92% → ↻ refresh blockhash`, each ending
in a green **FINALIZED** pill. RIGHT = `explorer.solana.com/block/429572113` showing that block on
mainnet. Title card: **"An AI recovered this failed Solana bundle on mainnet. Here's the block."**

**0:12–0:30 — Thesis (voiceover over the live rail; switch the tab to `/`).**
"On Solana, *sending* a transaction is easy. PrometheonOS is an execution **control plane**: it streams
the network, submits Jito bundles, and an AI agent operates the stack — pricing tips, and when a bundle
fails, diagnosing *why* and recovering it. Watch a run." Let the rail animate: tokens ride
Submitted → Processed → Confirmed → Finalized; point at the **OBSERVE** strip (the live tip-floor P50/P75/P95
the AI reads) and the honest **proof-replay** badge.

**0:30–1:05 — The two DIVERGENT recoveries (the proof).**
As the two injected faults hit, narrate the contrast — this is the whole pitch:
"Two failures, two *different* correct fixes. The under-tipped bundle: the AI **raises the tip**, keeps the
blockhash. The expired-blockhash bundle: it **refreshes the blockhash**, keeps the tip. Same engine,
different diagnosis, different lever — that's a real reasoned decision, not a script." **Hover a recovery
row** → its decision card spotlights in the AI Decision Timeline; read one line of the reasoning aloud.
Note the **AI OPERATOR** node pulsing amber with the live `retry {…}` it emitted.

**1:05–1:30 — Prove it's real (explorer).**
Click a green **FINALIZED** pill on the rail → it opens `explorer.solana.com/block/<slot>` → show the
block on mainnet. Narrate: "Every landing is a real, verifiable slot — this ran on mainnet." (Optional:
cut to `logs/lifecycle-log.md` → the **AI Recovery Chains** section threading attempt 1 → attempt 2.)

**1:30–1:50 — Honest scope (this builds trust, don't skip).**
Point at the **PRODUCT SURFACE** strip at the bottom: `submit(signedTx) → receipt{finalized_slot | reason}`.
Narrate: "Real users never touch this dashboard — it's the operator's control room. The product is the
integration: a signed tx in, a lifecycle receipt out." Flick the source toggle `proof-replay → live → sim`
to show the honest badges. Say the limits out loud: "Replay of a committed mainnet run — real data; the
floor clamps the AI's tip to a competitive minimum; leader-window is approximated via the RPC schedule."

**1:50–2:05 — Close.**
"Stream-confirmed lifecycle, dynamic tips, real-signal failure classification, and an AI that **owns the
recovery decision** — with the full reasoning persisted and every landing explorer-verifiable. That's
PrometheonOS." One beat on the architecture diagram (`docs/DIAGRAMS.md`) or the public arch-doc URL.

---

## Capture checklist (for the written submission)
- [ ] The `?t=34500` money-shot frame (both recoveries finalized, explorer slots) — your hero screenshot.
- [ ] A hovered recovery row with its **spotlighted** decision card (cross-highlight) — reasoning legible.
- [ ] The explorer block page for slot `429572113` (or `429572096`) with the URL in frame.
- [ ] `logs/lifecycle-log.md` — the **AI Recovery Chains** section (attempt 1 → attempt 2, linked).
- [ ] The dashboard with the **`proof-replay`** badge (honest provenance) + the receipt strip visible.

## Optional live flex (only if you want to show it's not pre-baked)
Toggle the badge to **LIVE**, then in a terminal run a small funded mainnet proof and let the rail update
from the real bus:
```bash
pnpm --filter @prometheon/ai-agent start                 # terminal 1 (verify it serves a decision first!)
NETWORK=mainnet ./scripts/run-proof.sh 12 low-tip:1,stale-blockhash:1   # terminal 2
```
Caveats: a live run is sparse/slow and the stale-blockhash injection pauses ~60–90 s for real expiry —
trim in edit. **Verify the agent actually serves a decision over NATS before spending SOL** (a dead agent
silently falls back to the deterministic policy). The `proof-replay` is the safe, repeatable hero; keep
the live run as a brief "and it's real" coda, not the spine.
```

## If something misbehaves on the day
- Rail empty / story not playing → you're at the start of the loop; refresh, or open `?t=34500`.
- Tokens bunched at FINALIZED with no detour → you're between recovery beats; use `?t=31000`–`34500`.
- Want the live loop slower for narration → scrub manually across `?t=` values instead of letting it run.

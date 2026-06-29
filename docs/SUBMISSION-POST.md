# PrometheonOS — submission post

> Paste-ready submission text. Pick the short or long version; fill the two `‹…›` link placeholders.

---

## One-liner

**PrometheonOS — an autonomous execution *control plane* for Solana.** It submits transactions as Jito
bundles, and when one fails on mainnet an AI agent reads *why* and recovers it to a finalized landing —
proven on-chain, watchable live.

---

## Short version (for a tight description field)

Most "smart transaction" tools sell a faster pipe or a tip number. PrometheonOS is the layer **above**
them: it streams Solana over Yellowstone gRPC, submits Jito bundles with dynamically-priced tips, tracks
each bundle's full lifecycle from the stream (no `getSignatureStatuses` polling), and when a bundle
doesn't land it **classifies the failure from real signals and an AI agent decides how to recover** —
then resubmits until it's finalized.

We proved it on **mainnet**: a committed run of **12 bundles landed + 2 deliberately-injected failures
that the AI recovered**, all finalized and explorer-verifiable. The two failures got two *different*
correct fixes — the under-tipped one: the AI **raised the tip**; the expired-blockhash one: the AI
**refreshed the blockhash** — which is the whole point: a real reasoned decision, not a scripted retry.

Watch it on the **Recovery Rail** dashboard (a 35-second clip is in the repo): bundles ride
Submitted→Processed→Confirmed→Finalized; the two faults visibly detour through the AI operator and
self-heal to slots you can click open on the explorer.

- **Repo:** https://github.com/Blessedbiello/PrometheonOS
- **Lifecycle log (the proof):** `logs/lifecycle-log.md` — 12 landed + 2 AI-recovered of 14, explorer-verifiable
- **Architecture doc:** ‹public URL›
- **Demo:** ‹video URL› (or `docs/assets/recovery-rail-demo.mp4` in the repo)

---

## Long version (the full writeup)

### The problem
On Solana, *sending* a transaction is the easy part. *Landing* a value-critical one during congestion —
a liquidation, an oracle update, a settlement — is where money is lost: the tx expires or gets crowded
out, and the usual "retry" is a dumb loop that re-sends the same expired blockhash with the same too-low
tip until it gives up. There's no read of *why* it failed and no economic logic on *how hard to try*.

### What PrometheonOS is
An **execution control plane**: it consumes the transport (Jito Block Engine) and the live tip-floor,
and adds the missing closed loop — observe → submit → confirm, and on a non-landing: **classify → AI
decide → recover**. The AI agent owns one real operational decision end-to-end: **Autonomous Retry with
Fault Injection**.

### Does it work? (proven on mainnet — judges can verify it)
A committed run, `logs/lifecycle-log.md`:
- **12 bundles landed + 2 AI-recovered failures of 14 submissions**, every landed bundle advancing
  `submitted → processed → confirmed → finalized`.
- Slots are real and clickable, e.g. block **[429572113](https://explorer.solana.com/block/429572113)**
  and **[429572096](https://explorer.solana.com/block/429572096)**.
- **15 real AI decisions** in the log (Groq `gpt-oss-120b` via an OpenAI-compatible endpoint) — zero
  deterministic fallback.
- The log opens with explicit **AI Recovery Chains** that thread each failed attempt → its real-signal
  classification → the recovered, finalized resubmission — so the recovery is cross-referenceable, not
  inferred. Cost: ~0.0025 SOL.

### The AI decision it owns (and why it isn't a gimmick)
On a non-landing the deterministic core classifies the failure from **real** signals (Jito/on-chain), and
asks the agent over NATS *how to recover*. Two failures in the run, **two divergent correct remedies**:
- `fee_too_low` (conf 0.80) → the agent **raises the tip**, keeps the blockhash.
- `expired_blockhash` (conf 0.92) → the agent **refreshes the blockhash**, keeps the tip.

The model's levers are load-bearing by construction: a **causal contract rejects any reply that omits the
`after.tip` / `after.refresh_blockhash` keys** — the engine acts on the model's exact values or the action
doesn't happen. (Honest nuance, stated below: the tip *value* is clamped to a competitive floor, so the
floor — not the model's exact number — sets most tips; the AI's provable, outcome-changing lever is the
**retry decision** itself.)

### Depth of integration (no hardcoded shortcuts)
- **Stream-confirmed lifecycle** from Yellowstone gRPC slot/tx-status — **zero `getSignatureStatuses`
  polling**; RPC is only a cross-check.
- **Dynamic tips** computed from the **live Jito tip-floor distribution** (P50/P75/P95) — never hardcoded;
  the floor is heavily skewed, so the engine anchors on the competitive band that actually lands.
- **Co-located, no-ALT tip**: the tip is a transfer in the *same* legacy tx as the strategy, so a
  non-landing pays **zero** tip and the tip account is never hidden in an Address Lookup Table.
- **Real-signal failure classification** (`expired_blockhash` / `fee_too_low` / …) from decoded Jito/on-chain
  data, not the injection tag.
- **Supervised reconnect + `from_slot` replay + drop-newest backpressure** on the stream.
- **One cross-language typed contract**: Rust `schemars` → JSON Schema → TS types, **CI fails on drift**.

Stack: a 10-crate Rust workspace + a TypeScript AI agent + a Next.js dashboard, over NATS, with
Postgres/TimescaleDB and Prometheus. ~187 Rust + 51 TS tests; CI runs fmt · clippy · tests · schema-drift.

### Watch it work — the "Recovery Rail" control room
The dashboard is one full-bleed instrument: each committed bundle is a token riding the four stations;
the two faults detour — a rose fault token, the AI's classified lever inline, the AI OPERATOR node pulsing
— and recover to a finalized landing whose slot **links to the explorer**. Hover a recovery row to
spotlight its decision + reasoning in the timeline. It defaults to an **honest `proof-replay`** of the
committed run (real on-chain data, real links — no faked liveness; toggle `live | simulated | proof-replay`).
A 35-second screen capture is committed at `docs/assets/recovery-rail-demo.mp4`.

### How it's actually used (the honest framing)
Real users never touch the dashboard — PrometheonOS is **headless infrastructure**. A keeper/bot/protocol
hands it a signed instruction and gets back a lifecycle receipt: `submit(signedTx) → receipt{ finalized_slot
| reason }`. The dashboard is the **operator's control room** (and this demo surface), not the product.

### The three README questions (answered from real behavior)
- **`processed`→`confirmed` delta = consensus health,** not RPC latency: time for a block we've already
  seen to gather a ≥⅔ stake-weighted optimistic vote; small/stable = healthy, widening = congestion/fork
  churn. Confirmed deltas in the committed run were ~0.4–1.8 s.
- **Never fetch a blockhash at `finalized`** for a time-sensitive tx: it's already ~31+ slots (~13 s) old,
  pre-spending ~20–30% of the fixed 150-block validity window for zero benefit. We fetch at `confirmed`.
- **If the Jito leader skips its slot** the bundle doesn't land, the **tip isn't paid** (co-located), the
  blockhash stays valid (a skip doesn't advance block height) — retry to the next Jito leader.

### Honest scope (what we do *not* claim)
- The proof payload is a benign **self-transfer harness**, not a real MEV/liquidation tx — the engine is
  payload-agnostic but demonstrated with the harness.
- The deterministic **200k-lamport competitive floor clamps the AI's tip**, so in this run the floor (not
  the model's exact number) set most tips — that's the safety envelope working; the AI's provable lever is
  the retry decision.
- **Leader window is approximated** via the RPC `getSlotLeaders` schedule + Block-Engine routing (Jito's
  searcher `getNextScheduledLeader` is auth-gated).
- **Retry-without-cancel:** Solana has no bundle cancel, so a given-up attempt and its retry can both land
  — bounded by the tip clamp; a durable-nonce single-flight guard is named future work.

### Run it
```bash
docker compose -f infra/docker-compose.yml up -d            # NATS · Postgres/Timescale · Prometheus
cargo test --workspace                                      # ~187 Rust tests, no network
pnpm --filter @prometheon/dashboard dev                     # the Recovery Rail → http://localhost:3000
# Funded mainnet proof (agent serving over NATS first):
NETWORK=mainnet ./scripts/run-proof.sh 12 low-tip:1,stale-blockhash:1
cargo run -p prometheon-telemetry --bin export-log          # → logs/lifecycle-log.{json,md}
```

### Links
- **Code (open source):** https://github.com/Blessedbiello/PrometheonOS
- **Architecture document (public):** ‹paste Notion / Google-Docs URL›
- **Lifecycle log (the proof):** `logs/lifecycle-log.md` (+ `.json`)
- **Demo video:** ‹paste link› — or `docs/assets/recovery-rail-demo.mp4`
- **Live dashboard:** `pnpm --filter @prometheon/dashboard dev` → http://localhost:3000 (`/?t=34500` = the self-heal)

---

## Optional one-line taglines (pick one for the title/hook)
- "An AI that operates a Solana transaction stack — and self-heals failures, on mainnet."
- "Execution intelligence for Solana: a bundle failed, an AI brought it back to a finalized landing — here's the block."
- "Not a faster pipe. The control plane that decides and recovers."

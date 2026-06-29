# PrometheonOS — Bundle Lifecycle Log

14 bundles · 12 landed · 2 failed. Slot numbers are verifiable on the Solana explorer.

## AI Recovery Chains

2 injected failures recovered to a **finalized landing on retry**. Each chain threads a failed attempt → its real-signal classification → the recovered resubmission; the retry decision that drove it (and its provider — AI agent or deterministic fallback) is in the **AI Decision Timeline** below.

**Logical bundle `b11`**

- attempt 1 · tip 1000 · sig `JL4u29…eCHk` → classified `fee_too_low` (conf 0.80) → AI retry decision ↓
- attempt 2 · tip 200000 · sig `3ZdXFV…FPY4` → **LANDED** [429572113](https://explorer.solana.com/block/429572113) (finalized, Δ 650 ms)

**Logical bundle `b12`**

- attempt 1 · tip 235000 · sig `5NUZvb…MtAa` → classified `expired_blockhash` (conf 0.92) → AI retry decision ↓
- attempt 2 · tip 235000 · sig `43bvsQ…HJAp` → **LANDED** [429572096](https://explorer.solana.com/block/429572096) (finalized, Δ 818 ms)

| # | Bundle | Tip (lamports) | First slot | Progression | Submit→Confirmed | Failure |
|---|--------|----------------|-----------|-------------|------------------|---------|
| 1 | `42e9b5…cf3f` | 200000 | [429571833](https://explorer.solana.com/block/429571833) | submitted→processed→confirmed→finalized | 2564 ms | — |
| 2 | `44805f…50d6` | 1000 | — | submitted | — | fee_too_low |
| 3 | `449f04…49f2` | 200000 | [429571828](https://explorer.solana.com/block/429571828) | submitted→processed→confirmed→finalized | 5021 ms | — |
| 4 | `47a259…955c` | 200000 | [429572113](https://explorer.solana.com/block/429572113) | submitted→processed→confirmed→finalized | 650 ms | — |
| 5 | `534e41…74af` | 200000 | [429571838](https://explorer.solana.com/block/429571838) | submitted→processed→confirmed→finalized | 356 ms | — |
| 6 | `5NUZvb…MtAa` | 235000 | — | submitted | — | expired_blockhash |
| 7 | `5fa37c…6e12` | 200000 | [429571880](https://explorer.solana.com/block/429571880) | submitted→processed→confirmed→finalized | 862 ms | — |
| 8 | `64f13e…aca8` | 200000 | [429571869](https://explorer.solana.com/block/429571869) | submitted→processed→confirmed→finalized | 343 ms | — |
| 9 | `99d74d…b209` | 200000 | [429571814](https://explorer.solana.com/block/429571814) | submitted→processed→confirmed→finalized | 3444 ms | — |
| 10 | `ad2f31…400f` | 200000 | [429571899](https://explorer.solana.com/block/429571899) | submitted→processed→confirmed→finalized | 512 ms | — |
| 11 | `bb76cc…ca1a` | 200000 | [429571890](https://explorer.solana.com/block/429571890) | submitted→processed→confirmed→finalized | 706 ms | — |
| 12 | `eaf0aa…608d` | 235000 | [429572096](https://explorer.solana.com/block/429572096) | submitted→processed→confirmed→finalized | 818 ms | — |
| 13 | `f8efc3…8cb9` | 200000 | [429571858](https://explorer.solana.com/block/429571858) | submitted→processed→confirmed→finalized | 444 ms | — |
| 14 | `fabbfd…90d0` | 200000 | [429571849](https://explorer.solana.com/block/429571849) | submitted→processed→confirmed→finalized | 896 ms | — |

## AI Decision Timeline

15 decisions recorded during the run, all by the **AI agent** — it proposes the tip and owns the autonomous-retry decision (the deterministic core clamps each tip to the competitive floor; provider is shown per entry).

**1. [timing]** submit — confidence 0.93 · openai · 2026-06-29T01:57:24.354Z

> CongestionScore is 0 (no network congestion) and only 1 slot remains before leader change, so submitting now avoids added latency. No evidence the upcoming leader is a Jito validator, so no benefit in holding.

`before: null  →  after: {"hold":false}`

**2. [tip]** set tip — confidence 0.85 · openai · 2026-06-29T01:57:25.419Z

> CongestionScore is 0 (low load). Tip floor at the 75th percentile is 10035 lamports and at the 95th percentile is 100000 lamports. To balance cost and inclusion probability, we choose a tip slightly above the 75th percentile, well within the P75‑P95 band, at 12000 lamports.

`before: null  →  after: {"tip":12000}`

**3. [tip]** set tip — confidence 0.86 · openai · 2026-06-29T01:57:29.577Z

> CongestionScore=0 indicates low load; tipFloorP75Lamports=10035 gives reliable inclusion. Choosing a tip modestly above P75 (12000 lamports) stays in the P75‑P95 band while conserving cost.

`before: null  →  after: {"tip":12000}`

**4. [tip]** Set Jito tip — confidence 0.87 · openai · 2026-06-29T01:57:34.064Z

> CongestionScore is 0 (low), tipFloorP50Lamports=5542, tipFloorP75Lamports=10035, tipFloorP95Lamports=100000. With low congestion we can stay at the lower end of the reliable band, so we choose the P75 floor (10035 lamports) to balance cost and inclusion probability.

`before: null  →  after: {"tip":10035}`

**5. [tip]** set tip — confidence 0.85 · openai · 2026-06-29T01:57:38.125Z

> CongestionScore is 0 (no congestion). To achieve reliable inclusion we must stay within the P75‑P95 band. With low congestion we can target the lower side of that band, selecting a tip modestly above tipFloorP75Lamports (~10k) but well below P95. Chosen tip ~28,000 lamports balances cost and landing probability.

`before: null  →  after: {"tip":28028}`

**6. [tip]** set jito tip — confidence 0.85 · openai · 2026-06-29T01:57:42.115Z

> CongestionScore is 0, indicating a clear block space. The P75 floor is 10035 lamports, well above the noise floor, and P95 is 100000 lamports. With no congestion we can target the lower end of the reliable band, just above P75, to balance cost and inclusion probability.

`before: null  →  after: {"tip":11000}`

**7. [tip]** set tip — confidence 0.85 · openai · 2026-06-29T01:57:46.428Z

> CongestionScore is 0, indicating low network load. The tip floor at the 75th percentile is 10035 lamports, which should reliably land in these conditions. Selecting a tip modestly above this level (~12000 lamports) provides a safety margin without overpaying.

`before: null  →  after: {"tip":12000}`

**8. [tip]** set tip — confidence 0.85 · openai · 2026-06-29T01:57:50.574Z

> Low congestion (score 0) lets us stay near the lower end of the reliable band. Tip floor P75 is 10035 lamports and P95 is 100000 lamports; picking ~19 k lamports (~10 % above P75) balances cost and inclusion probability.

`before: null  →  after: {"tip":19000}`

**9. [tip]** setTip — confidence 0.78 · openai · 2026-06-29T01:57:54.748Z

> CongestionScore is 0 (very low), so a tip near the lower end of the P75‑P95 band is sufficient. Using the live floor values, we pick a tip of 32,526 lamports, which is above the P75 floor (10,035) but well below the P95 floor (100,000), matching low‑congestion conditions.

`before: null  →  after: {"tip":32526}`

**10. [tip]** setTip — confidence 0.86 · openai · 2026-06-29T01:57:58.873Z

> CongestionScore is 0, indicating minimal network load. The tip floor P75 is 10035 lamports and P95 is 100000 lamports. With low congestion we can stay near the lower end of the reliable band; a tip of 11000 lamports comfortably exceeds P75 while remaining far below P95, balancing cost and landing probability.

`before: null  →  after: {"tip":11000}`

**11. [tip]** setTip — confidence 0.85 · openai · 2026-06-29T01:58:02.864Z

> CongestionScore is 0 (minimal), so a tip near the lower end of the reliable band suffices. Using the P75 floor (10035 lamports) plus a modest safety margin yields ~12000 lamports, staying within the P75‑P95 window while avoiding excess cost.

`before: null  →  after: {"tip":12000}`

**12. [tip]** set tip — confidence 0.85 · openai · 2026-06-29T01:58:06.874Z

> CongestionScore is 0 (no congestion), so a tip just above the P75 floor is sufficient. P75 is 10035 lamports; choosing 20000 lamports places us comfortably in the P75–P95 band without overspending on the P95 extreme, maximizing cost‑efficiency while maintaining high inclusion probability.

`before: null  →  after: {"tip":20000}`

**13. [tip]** set tip to 10035 lamports — confidence 0.88 · openai · 2026-06-29T01:58:11.112Z

> CongestionScore is 0, indicating no current network pressure. The tipFloorP75Lamports is 10035, placing us at the start of the reliable P75‑P95 band. With zero congestion we can target the lower bound of this band to minimize cost while still maintaining a good inclusion chance.

`before: null  →  after: {"tip":10035}`

**14. [retry]** RefreshBlockhashAndResubmit — confidence 0.97 · openai · 2026-06-29T01:59:20.190Z

> FailureClass=ExpiredBlockhash indicates the recent blockhash is no longer valid. Telemetry shows lastTipLamports=235000 which comfortably exceeds tipFloorP50Lamports=5542, so the current tip is sufficient. Refreshing the blockhash and resubmitting with the same tip is safe and likely to succeed.

`before: null  →  after: {"refresh_blockhash":true,"tip":235000}`

**15. [retry]** retry — confidence 0.92 · openai · 2026-06-29T01:59:26.667Z

> FailureClass=FeeTooLow; current tip 1000 lamports is below tipFloorP50 5542 lamports, so transaction was rejected for insufficient fee. No blockhash expiration indicated, so we keep the same blockhash and increase tip above the floor.

`before: null  →  after: {"refresh_blockhash":false,"tip":5600}`


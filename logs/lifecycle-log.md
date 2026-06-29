# PrometheonOS — Bundle Lifecycle Log

14 bundles · 12 landed · 2 failed. Slot numbers are verifiable on the Solana explorer.

| # | Bundle | Tip (lamports) | First slot | Progression | Submit→Confirmed | Failure |
|---|--------|----------------|-----------|-------------|------------------|---------|
| 1 | `04ad63…8747` | 1000 | — | submitted | — | fee_too_low |
| 2 | `07523f…94b7` | 200000 | [429560253](https://explorer.solana.com/block/429560253) | submitted→processed→confirmed→finalized | 599 ms | — |
| 3 | `25df30…9627` | 200000 | [429560211](https://explorer.solana.com/block/429560211) | submitted→processed→confirmed→finalized | 1786 ms | — |
| 4 | `2a23b9…4331` | 200000 | [429560276](https://explorer.solana.com/block/429560276) | submitted→processed→confirmed→finalized | 1488 ms | — |
| 5 | `326131…e00d` | 250000 | [429560175](https://explorer.solana.com/block/429560175) | submitted→processed→confirmed→finalized | 793 ms | — |
| 6 | `3b08e5…ddba` | 200000 | [429560218](https://explorer.solana.com/block/429560218) | submitted→processed→confirmed→finalized | 304 ms | — |
| 7 | `3xmLGp…HjjC` | 200000 | — | submitted | — | expired_blockhash |
| 8 | `56768e…d665` | 200000 | [429560266](https://explorer.solana.com/block/429560266) | submitted→processed→confirmed→finalized | 1758 ms | — |
| 9 | `5e9800…82af` | 200000 | [429560496](https://explorer.solana.com/block/429560496) | submitted→processed→confirmed→finalized | 5396 ms | — |
| 10 | `640d8a…ca7b` | 200000 | [429560197](https://explorer.solana.com/block/429560197) | submitted→processed→confirmed→finalized | 441 ms | — |
| 11 | `659a16…e6dc` | 286080 | [429560230](https://explorer.solana.com/block/429560230) | submitted→processed→confirmed→finalized | 1047 ms | — |
| 12 | `8f2ebe…ff4a` | 200000 | [429560473](https://explorer.solana.com/block/429560473) | submitted→processed→confirmed→finalized | 3094 ms | — |
| 13 | `d5755a…0f2f` | 200000 | [429560186](https://explorer.solana.com/block/429560186) | submitted→processed→confirmed→finalized | 775 ms | — |
| 14 | `f25d4a…ebc2` | 200000 | [429560244](https://explorer.solana.com/block/429560244) | submitted→processed→confirmed→finalized | 818 ms | — |

## AI Decision Timeline

15 AI decisions recorded during the run (the agent owns the tip + autonomous-retry decisions).

**1. [timing]** submit now — confidence 0.85 · openai · 2026-06-29T00:39:04.841Z

> CongestionScore is 0 (no network backlog) and slotsUntilLeaderChange is 1, so the current leader will step down imminently. Submitting now avoids the latency of waiting for the next leader slot, which may not be a Jito leader.

`before: null  →  after: {"hold":false}`

**2. [tip]** setTip — confidence 0.85 · openai · 2026-06-29T00:39:06.468Z

> CongestionScore is 0, indicating minimal network load. To achieve reliable inclusion we stay within the reliable band (P75–P95). Choosing a tip modestly above the P75 floor (107,600 lamports) provides high inclusion probability without overpaying. A tip of 250,000 lamports balances cost and reliability.

`before: null  →  after: {"tip":250000}`

**3. [tip]** setTip — confidence 0.85 · openai · 2026-06-29T00:39:10.867Z

> CongestionScore is 0 (no congestion). Tip floor P75 is 107600 lamports; to ensure inclusion while avoiding excess cost we set tip slightly above P75 at 120000 lamports, staying within the P75‑P95 band.

`before: null  →  after: {"tip":120000}`

**4. [tip]** set tip to 150000 lamports — confidence 0.92 · openai · 2026-06-29T00:39:15.388Z

> CongestionScore is 0 (no congestion). To achieve reliable inclusion we stay within the P75‑P95 band. P75 is 107600 lamports; choosing 150000 puts us comfortably above P75 while far below the P95 noise floor, balancing cost and probability.

`before: null  →  after: {"tip":150000}`

**5. [tip]** set tip — confidence 0.85 · openai · 2026-06-29T00:39:19.616Z

> CongestionScore is 0 (no congestion), so a tip anchored at the P75 level is sufficient; P50 is too low to reliably land, and P95 is unnecessary given the empty mempool.

`before: null  →  after: {"tip":107600}`

**6. [tip]** set tip — confidence 0.78 · openai · 2026-06-29T00:39:23.736Z

> CongestionScore is 0, indicating very low network load. The P75 floor is 107,600 lamports, and the P95 floor is 1,000,000 lamports. To secure inclusion without overpaying, we target a tip modestly above the P75 level but well below the P95 ceiling, selecting 150,000 lamports.

`before: null  →  after: {"tip":150000}`

**7. [tip]** set Jito tip — confidence 0.85 · openai · 2026-06-29T00:39:28.152Z

> CongestionScore is 0 (no network pressure). To achieve reliable inclusion we must tip within the P75–P95 band. With low congestion we target the lower side of that band, around 20% above the P75 floor (107600 lamports), yielding ~286080 lamports, well below the P95 noise floor but above the P75 threshold.

`before: null  →  after: {"tip":286080}`

**8. [tip]** setTip — confidence 0.85 · openai · 2026-06-29T00:39:33.627Z

> CongestionScore is 0 (no network pressure). To balance cost and inclusion probability we anchor at the P75 floor, which is sufficient when traffic is light, rather than pushing toward P95 which is needed only under higher congestion.

`before: null  →  after: {"tip":107600}`

**9. [tip]** set tip — confidence 0.85 · openai · 2026-06-29T00:39:38.223Z

> CongestionScore is 0 (minimal congestion). To ensure reliable inclusion without overpaying, we target the lower part of the P75‑P95 band. Using tipFloorP75Lamports (107600) and moving ~10% toward tipFloorP95Lamports yields a tip of ~196800 lamports, comfortably above the P75 floor while staying far below the P95 ceiling.

`before: null  →  after: {"tip":196800}`

**10. [tip]** set tip — confidence 0.92 · openai · 2026-06-29T00:39:42.244Z

> CongestionScore is 0, indicating no network pressure. To achieve reliable inclusion we target the 75th‑percentile floor, which is the start of the reliable band. Tip set to tipFloorP75Lamports (107600 lamports).

`before: null  →  after: {"tip":107600}`

**11. [tip]** set tip — confidence 0.85 · openai · 2026-06-29T00:39:46.414Z

> Congestion score is 0 (no congestion). To achieve reliable inclusion we target the P75–P95 band; with low congestion we stay near the lower end. Tip set just above the P75 floor (107600 lamports) at 115000 lamports.

`before: null  →  after: {"tip":115000}`

**12. [tip]** set tip — confidence 0.92 · openai · 2026-06-29T00:39:50.445Z

> CongestionScore is 0 indicating minimal network load; the tipFloorP75Lamports (107600) is sufficient to achieve reliable inclusion while staying well above the noisy P50 floor.

`before: null  →  after: {"tip":107600}`

**13. [tip]** setTip — confidence 0.86 · openai · 2026-06-29T00:39:54.813Z

> CongestionScore is 0 (no congestion), so we can stay near the lower end of the reliable band. P75 floor is 107600 lamports; picking a tip well above P75 but far below P95 conserves funds while still giving a high inclusion probability.

`before: null  →  after: {"tip":330700}`

**14. [retry]** Refresh blockhash and resubmit — confidence 0.94 · openai · 2026-06-29T00:41:06.795Z

> FailureClass=ExpiredBlockhash indicates the recent blockhash is no longer valid. Telemetry shows a high lastTipLamports (200000) well above the tipFloorP50 (21402), so the existing tip is sufficient. We must refresh the blockhash and reuse the current tip for a retry.

`before: null  →  after: {"refresh_blockhash":true,"tip":200000}`

**15. [retry]** retry with increased tip — confidence 0.95 · openai · 2026-06-29T00:41:13.162Z

> FailureClass FeeTooLow indicates the submitted fee (1000 lamports) is below the network's median tip floor (21402 lamports). No blockhash expiration reported, so we keep the current blockhash and raise the tip to meet the floor.

`before: null  →  after: {"refresh_blockhash":false,"tip":21402}`


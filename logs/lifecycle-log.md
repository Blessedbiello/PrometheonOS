# PrometheonOS — Bundle Lifecycle Log

14 bundles · 12 landed · 2 failed. Slot numbers are verifiable on the Solana explorer.

| # | Bundle | Tip (lamports) | First slot | Progression | Submit→Confirmed | Failure |
|---|--------|----------------|-----------|-------------|------------------|---------|
| 1 | `0657d3…78e3` | 200000 | [429547860](https://explorer.solana.com/block/429547860) | submitted→processed→confirmed→finalized | 1254 ms | — |
| 2 | `0aaccb…fa67` | 200000 | [429548126](https://explorer.solana.com/block/429548126) | submitted→processed→confirmed→finalized | 495 ms | — |
| 3 | `40d371…f325` | 200000 | [429547908](https://explorer.solana.com/block/429547908) | submitted→processed→confirmed→finalized | 1662 ms | — |
| 4 | `580ab8…2080` | 200000 | [429547928](https://explorer.solana.com/block/429547928) | submitted→processed→confirmed→finalized | 455 ms | — |
| 5 | `5MphXk…epuj` | 200000 | — | submitted | — | bundle_failure |
| 6 | `6fb748…aca6` | 200000 | [429547828](https://explorer.solana.com/block/429547828) | submitted→processed→confirmed→finalized | 600 ms | — |
| 7 | `7d2bd8…71ee` | 200000 | [429547875](https://explorer.solana.com/block/429547875) | submitted→processed→confirmed→finalized | 836 ms | — |
| 8 | `88afc4…22d5` | 200000 | [429547887](https://explorer.solana.com/block/429547887) | submitted→processed→confirmed→finalized | 774 ms | — |
| 9 | `8d5fc7…7ed4` | 200000 | [429547916](https://explorer.solana.com/block/429547916) | submitted→processed→confirmed→finalized | 451 ms | — |
| 10 | `98ec1a…2c17` | 200000 | [429548143](https://explorer.solana.com/block/429548143) | submitted→processed→confirmed→finalized | 518 ms | — |
| 11 | `9bacef…3578` | 1000 | — | submitted | — | bundle_failure |
| 12 | `df5cad…eaf5` | 200000 | [429547850](https://explorer.solana.com/block/429547850) | submitted→processed→confirmed→finalized | 1236 ms | — |
| 13 | `e1f076…3c53` | 200000 | [429547838](https://explorer.solana.com/block/429547838) | submitted→processed→confirmed→finalized | 729 ms | — |
| 14 | `ef7e78…0878` | 200000 | [429547897](https://explorer.solana.com/block/429547897) | submitted→processed→confirmed→finalized | 920 ms | — |

## AI Decision Timeline

15 AI decisions recorded during the run (the agent owns the tip + autonomous-retry decisions).

**1. [timing]** hold — confidence 0.62 · openai · 2026-06-28T23:16:04.673Z

> Current slot shows zero congestion and only 1 slot until leader rotation; waiting one slot may capture a Jito leader if scheduled, improving inclusion speed without added risk.

`before: null  →  after: {"hold":true}`

**2. [tip]** set_tip — confidence 0.85 · openai · 2026-06-28T23:16:06.450Z

> CongestionScore is 0 (no congestion), so we can target the lower end of the reliable band. P75 tip floor is 20269 lamports; P95 is 268263 lamports. To balance cost and landing probability we pick a tip slightly above P75, at 22000 lamports, staying within the P75‑P95 range but far from the noisy P50 floor.

`before: null  →  after: {"tip":22000}`

**3. [tip]** set Jito tip — confidence 0.90 · openai · 2026-06-28T23:16:10.505Z

> CongestionScore is 0, indicating low network pressure. To balance cost and inclusion probability we stay in the P75‑P95 band but lean toward the lower end since congestion is minimal. tipFloorP75Lamports is 20269, so a tip slightly above that (21000 lamports) gives a good chance of landing without overpaying.

`before: null  →  after: {"tip":21000}`

**4. [tip]** set tip to moderate level — confidence 0.90 · openai · 2026-06-28T23:16:14.806Z

> CongestionScore is 0 (no congestion). To balance cost and inclusion probability we stay near the lower end of the reliable band (P75–P95). Tip is set just above tipFloorP75Lamports (20269) to capture the P75‑P95 advantage without overpaying.

`before: null  →  after: {"tip":25000}`

**5. [tip]** set tip to P75 floor — confidence 0.92 · openai · 2026-06-28T23:16:18.762Z

> CongestionScore is 0 (no backlog), so a tip in the lower part of the P75‑P95 band is sufficient. Using tipFloorP75Lamports (20269) gives a strong inclusion chance without overspending, as the P50 floor (9360) is just noise.

`before: null  →  after: {"tip":20269}`

**6. [tip]** set tip — confidence 0.78 · openai · 2026-06-28T23:16:23.611Z

> CongestionScore is 0 (no congestion). To land reliably we stay in the P75‑P95 band but lean toward the lower end. tipFloorP75=20269 and tipFloorP95=268263, so a tip around 50 000 lamports is comfortably above P75 yet far below P95, balancing cost and inclusion probability.

`before: null  →  after: {"tip":50000}`

**7. [tip]** setTip — confidence 0.88 · openai · 2026-06-28T23:16:29.655Z

> CongestionScore is 0 (no congestion), so we can stay near the lower end of the reliable band. tipFloorP75Lamports=20269 and tipFloorP95Lamports=268263. Choosing 25000 lamports gives a comfortable margin above P75 while conserving cost.

`before: null  →  after: {"tip":25000}`

**8. [tip]** setTip — confidence 0.85 · openai · 2026-06-28T23:16:33.471Z

> CongestionScore is 0 (no network pressure). The P75 tip floor is 20269 lamports and the P95 is 268263 lamports. To achieve reliable inclusion while avoiding unnecessary cost, we target the lower end of the reliable band, slightly above P75. A tip of 25000 lamports balances cost and landing probability under current low congestion.

`before: null  →  after: {"tip":25000}`

**9. [tip]** set tip — confidence 0.85 · openai · 2026-06-28T23:16:37.538Z

> CongestionScore is 0, indicating a quiet mempool. The landed-tip distribution shows P50 (~9360) rarely lands, so we must target the P75‑P95 band. With no congestion we can stay near the lower end of that band; P75 is 20269 lamports. Choosing a tip slightly above P75 (21000 lamports) balances cost and inclusion probability.

`before: null  →  after: {"tip":21000}`

**10. [tip]** set jito tip — confidence 0.92 · openai · 2026-06-28T23:16:42.127Z

> CongestionScore is 0, indicating minimal network pressure. To ensure reliable inclusion we must stay within the P75‑P95 band. With no congestion we can target the lower portion of that band, about 10% above the P75 floor (20269 lamports), resulting in roughly 45 k lamports. This balances cost against landing probability.

`before: null  →  after: {"tip":45068}`

**11. [tip]** set_tip — confidence 0.85 · openai · 2026-06-28T23:16:46.549Z

> CongestionScore is 0 indicating minimal network load. To achieve reliable inclusion while conserving lamports, we target the lower end of the reliable band. The P75 floor is 20269 lamports; a tip around 50000 lamports stays comfortably above P75 yet far below P95, balancing cost and success probability.

`before: null  →  after: {"tip":50000}`

**12. [tip]** set tip — confidence 0.85 · openai · 2026-06-28T23:16:50.974Z

> CongestionScore is 0 (no congestion). The reliable landing band is tipFloorP75Lamports‑tipFloorP95Lamports (20269‑268263). With no pressure we can stay at the lower edge of this band (P75) to save cost while still having high inclusion probability.

`before: null  →  after: {"tip":20269}`

**13. [tip]** set tip — confidence 0.86 · openai · 2026-06-28T23:16:55.404Z

> CongestionScore is 0 (no congestion), so we can target the lower end of the reliable band. Tip is set just above the P75 floor (20269) to ensure inclusion without excessive cost.

`before: null  →  after: {"tip":22000}`

**14. [retry]** retry_submission — confidence 0.78 · openai · 2026-06-28T23:18:06.832Z

> BundleFailure indicates a transient bundle processing issue; telemetry shows blockhash still valid (no expiration flag) and tip (200000) far exceeds floor (9360), so we can retry unchanged

`before: null  →  after: {"refresh_blockhash":false,"tip":200000}`

**15. [retry]** RetryWithHigherTip — confidence 0.85 · openai · 2026-06-28T23:18:13.485Z

> BundleFailure indicates the bundle was rejected, typically due to insufficient tip. Telemetry shows last tip 1000 lamports while the 50th percentile tip floor is 9360 lamports. Raising the tip to meet the floor improves acceptance probability. No blockhash expiration is reported, so the blockhash can be reused.

`before: null  →  after: {"refresh_blockhash":false,"tip":9360}`


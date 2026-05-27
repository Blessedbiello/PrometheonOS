# Dashboard Wireframes _(Phase 7 spec)_

Operational telemetry UI — the "visual proof of sophistication." Realtime via NATS→WebSocket;
historical panels read Postgres. Dark, dense, ops-console aesthetic (think a trading-desk monitor).

## Layout

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  PrometheonOS   ● live   network: mainnet   slot 298,431,204   provider: claude│
├───────────────┬───────────────────────────────────┬──────────────────────────┤
│ NETWORK HEALTH│ SLOT / LEADER STREAM              │ AI DECISION TIMELINE      │
│ congestion ▓▓▓░│ ▸ slot 298431204  L: Jito✓  120ms │ 14:02:11 TIP 12k→18k      │
│ 0.74 ↑        │ ▸ slot 298431203  L: ---   skipped│  "congestion 0.74↑, 50th  │
│ stability 0.42│ ▸ slot 298431202  L: Jito✓   98ms │   floor 14.2k, last 3 @12k│
│ landing  0.71 │ next Jito leader: +3 slots        │   missed" conf 0.81       │
│ expiry   0.23 │                                   │ 14:02:09 HOLD 2 slots     │
│ p→c Δ 612ms   │                                   │ 14:01:55 RETRY blockhash  │
├───────────────┴───────────────────────────────────┴──────────────────────────┤
│ ACTIVE BUNDLES & LIFECYCLE                                                     │
│ bundle_id   tip     stage progression                       slot      latency │
│ a3f9…  18k  Sub━▶Proc━▶Conf━▶[Final]                       298431202  c:612ms │
│ b1c7…  14k  Sub━▶Proc━▶[Conf]…                              298431204  p:301ms │
│ c0d2…  12k  Sub━▶✗ EXPIRED → retrying (attempt 2/3)         —         —       │
├──────────────────────────────────────┬─────────────────────────────────────────┤
│ EXECUTION QUALITY (rolling 5m)        │ RETRIES & FAILURES                      │
│ cost/landing  21,400 lamports         │ ⟳ retry_success_rate 0.83               │
│ tip_efficiency 0.0000047 land/lamport │ ✗ expired_blockhash  4   (2 injected)   │
│ landing prob by tip tier  ▁▃▅▇        │ ✗ fee_too_low        2                  │
└──────────────────────────────────────┴─────────────────────────────────────────┘
```

## Panels
1. **Header** — live/connected, network, current slot, active LLM provider.
2. **Network Health** — gauges: `congestion_score`, `slot_stability_score`,
   `bundle_landing_probability`, `expiry_risk_score`, and the live `processed→confirmed` delta.
3. **Slot / Leader stream** — scrolling slots with leader identity, Jito✓ flag, skipped markers,
   slot time; "next Jito leader in N slots."
4. **AI Decision Timeline** — reverse-chron decisions with type, action (before→after), reasoning
   excerpt, confidence; click to expand full trace + inputs considered.
5. **Active Bundles & Lifecycle** — per-bundle stage progression bar with per-stage latency, slot,
   tip; failed/retrying rows highlighted.
6. **Execution Quality** — `cost_per_successful_landing`, `tip_efficiency_ratio`, landing-prob
   histogram by tip tier (rolling window).
7. **Retries & Failures** — counts by class (marking fault-injected vs organic), `retry_success_rate`.

## Demo framing
During the demo we trigger a blockhash-expiry injection and the operator watches a bundle go
`EXPIRED → retrying`, an AI `RETRY` decision appear in the timeline with its reasoning, then a fresh
bundle land — and open a Solana explorer on the logged slot to prove it's real.

# PrometheonOS — Real AI Decision Trace

Captured from the live `openai` provider (no mock). These are real model decisions over the exact tip/retry contexts the Rust core sends; each must satisfy the causal contract (`after.tip` for tip, `after.{tip,refresh_blockhash}` for retry) or the agent rejects it.

## tip decision

**Action:** Set Jito tip

**Confidence:** 0.78 · **Provider:** openai · **Latency:** 1861 ms

**Reasoning:** P50 floor is 14,200 lamports. CongestionScore 0.62 and recent failure suggest modest upward pressure; applying a ~12% bump (congestionScore × 0.2) yields ~1,6000 lamports, which balances cost vs landing probability (0.7) and improves chance of landing without overspending.

```json
{
  "before": null,
  "after": {
    "tip": 16000
  },
  "inputs": {
    "congestionScore": 0.62,
    "slotStabilityScore": 0.88,
    "bundleLandingProbability": 0.7,
    "tipFloorP50Lamports": 14200,
    "avgConfirmedLatencyMs": 920,
    "recentFailures": 1,
    "lastTipLamports": 12000
  }
}
```

## retry decision

**Action:** Refresh blockhash and resubmit with increased tip

**Confidence:** 0.94 · **Provider:** openai · **Latency:** 1156 ms

**Reasoning:** Failure class is ExpiredBlockhash, so the recent blockhash is no longer valid. Telemetry shows a tip floor of 16800 lamports and current tip 14500, plus moderate congestion (0.74). Refreshing the blockhash and raising the tip to at least the floor ensures the transaction is accepted.

```json
{
  "before": null,
  "after": {
    "refresh_blockhash": true,
    "tip": 16800
  },
  "inputs": {
    "failureClass": "ExpiredBlockhash",
    "attempt": 1,
    "lastTipLamports": 14500,
    "congestionScore": 0.74,
    "tipFloorP50Lamports": 16800,
    "slotStabilityScore": 0.71
  }
}
```


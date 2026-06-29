# PrometheonOS — Real AI Decision Trace

Captured from the live `openai` provider (no mock). These are real model decisions over the exact tip/retry contexts the Rust core sends; each must satisfy the causal contract (`after.tip` for tip, `after.{tip,refresh_blockhash}` for retry) or the agent rejects it.

## tip decision

**Action:** Set Jito tip

**Confidence:** 0.78 · **Provider:** openai · **Latency:** 2246 ms

**Reasoning:** CongestionScore is 0.62 and current landing probability 0.7, indicating moderate‑high network load. The P50 floor is 14,200 lamports; estimating the 75‑95 percentile band as roughly 21k‑28k lamports, we target the upper end to boost inclusion. Selected tip=26,000 lamports.

```json
{
  "before": null,
  "after": {
    "tip": 26000
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

**Action:** retry_with_refresh_and_updated_tip

**Confidence:** 0.93 · **Provider:** openai · **Latency:** 1288 ms

**Reasoning:** Failure class ExpiredBlockhash requires a fresh recent blockhash; congestionScore 0.74 and tipFloorP50Lamports 16800 indicate current tip is below floor, so increase tip to 18000 lamports to improve priority.

```json
{
  "before": null,
  "after": {
    "refresh_blockhash": true,
    "tip": 18000
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


/**
 * Deterministic mock provider for tests and offline runs. It produces schema-valid, context-aware
 * decisions without any API call, so the full agent flow is testable without keys or network.
 */
import type { DecisionRequest, LlmDecision, LlmProvider } from "./types.js";

function num(ctx: Record<string, unknown>, key: string, fallback: number): number {
  const v = ctx[key];
  return typeof v === "number" && Number.isFinite(v) ? v : fallback;
}

export class MockProvider implements LlmProvider {
  readonly name = "mock";

  async decide(req: DecisionRequest): Promise<LlmDecision> {
    const ctx = req.context;
    const congestion = num(ctx, "congestionScore", 0);

    switch (req.decisionType) {
      case "tip": {
        const floor = num(ctx, "tipFloorP50Lamports", 10_000);
        const tip = Math.round(floor * (1 + 0.5 * congestion));
        return {
          action: `tip ${tip} lamports`,
          reasoning: `median floor ${floor} lamports scaled by congestion ${congestion}.`,
          confidence: 0.7,
          before: { tip: floor },
          after: { tip },
        };
      }
      case "timing": {
        const hold = congestion > 0.7;
        return {
          action: hold ? "hold submission" : "submit now",
          reasoning: hold
            ? `congestion ${congestion} is elevated; hold for a healthier leader window.`
            : `congestion ${congestion} is acceptable; submit now.`,
          confidence: 0.65,
        };
      }
      case "retry": {
        const floor = num(ctx, "tipFloorP50Lamports", 10_000);
        const last = num(ctx, "lastTipLamports", floor);
        const tip = Math.round(Math.max(last, floor) * (1 + 0.5 * congestion));
        return {
          action: `refresh blockhash, re-price ${last}->${tip}`,
          reasoning: "blockhash expiry is retryable; refresh and re-price before resubmitting.",
          confidence: 0.8,
          before: { tip: last, blockhash: "stale" },
          after: { refresh_blockhash: true, tip },
        };
      }
    }
  }
}

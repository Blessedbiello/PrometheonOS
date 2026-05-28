/**
 * Pure formatters for the ops dashboard. Keep deterministic and unit-tested.
 */

const LAMPORTS_PER_SOL = 1_000_000_000;

/** Lamports formatted with thousands separators. */
export function formatLamports(n: number): string {
  return `${Math.round(n).toLocaleString("en-US")} lamports`;
}

/** Convert lamports to a SOL string with 4-6 sig figs (auto). */
export function lamportsToSol(lamports: number, decimals = 6): string {
  const sol = lamports / LAMPORTS_PER_SOL;
  return `${sol.toFixed(decimals)} SOL`;
}

/** Duration in ms → "612ms" or "1.20s" (>=1000ms) or "13.30s" (>=10s). */
export function formatMs(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || Number.isNaN(ms)) return "—";
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(ms >= 10_000 ? 2 : 2)}s`;
}

/** Fraction in [0,1] → "74%" (no decimal). */
export function formatPct(x: number | null | undefined): string {
  if (x === null || x === undefined || Number.isNaN(x)) return "—";
  return `${Math.round(x * 100)}%`;
}

/** Truncate a long id like `a3f9b6e21c0d2` → `a3f9b6…c0d2`. */
export function truncId(id: string, head = 6, tail = 4): string {
  if (id.length <= head + tail + 1) return id;
  return `${id.slice(0, head)}…${id.slice(-tail)}`;
}

/** Format a slot number with thousands separators. */
export function formatSlot(slot: number): string {
  return slot.toLocaleString("en-US");
}

/** Stage label + status color tag for visual coding. */
export function stageColor(stage: string): "green" | "red" | "yellow" | "blue" | "gray" {
  switch (stage) {
    case "finalized":
      return "green";
    case "confirmed":
      return "blue";
    case "processed":
      return "yellow";
    case "submitted":
      return "gray";
    case "failed":
    case "expired":
    case "dropped":
      return "red";
    default:
      return "gray";
  }
}

/** Confidence bar segments out of 5 for compact display. */
export function confidenceBars(confidence: number): string {
  const filled = Math.max(0, Math.min(5, Math.round(confidence * 5)));
  return "█".repeat(filled) + "░".repeat(5 - filled);
}

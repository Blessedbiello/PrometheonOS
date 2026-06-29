"use client";

import type { BundleRow } from "@/lib/types";

/**
 * Product-surface receipt strip. Real users never touch the dashboard — the product is an integration:
 * a keeper/bot/protocol hands PrometheonOS a signed instruction and gets back a lifecycle receipt. This
 * strip renders that literal contract, rolling the real values the rail is animating above it, so a
 * judge never mistakes the operator console for the product.
 */
export function ReceiptStrip({ bundles }: { bundles: BundleRow[] }) {
  // The most recently resolved bundle (finalized, else the latest failed attempt).
  const resolved = [...bundles].reverse();
  const landedB = resolved.find((b) => b.stage === "finalized" || b.stage === "confirmed");
  const failedB = resolved.find((b) => b.failure_class != null);
  const b = landedB ?? failedB ?? bundles[bundles.length - 1];

  let receipt: string;
  if (!b) {
    receipt = `receipt{ pending }`;
  } else if (b.stage === "finalized" || b.stage === "confirmed") {
    const attempts = (b.attempt ?? 1) > 1 ? `, attempts: ${b.attempt}` : "";
    const recovered = (b.attempt ?? 1) > 1 ? `, recovered: true` : "";
    receipt = `receipt{ landed: true, finalized_slot: ${b.slot?.toLocaleString() ?? "—"}${attempts}${recovered} }`;
  } else {
    receipt = `receipt{ landed: false, reason: "${b.failure_class ?? "pending"}", retrying… }`;
  }

  const landed = b && (b.stage === "finalized" || b.stage === "confirmed");

  return (
    <footer className="sticky bottom-0 z-10 border-t border-zinc-900 bg-[#0b0b0c]/95 px-5 py-2 backdrop-blur">
      <div className="mono flex items-center gap-2 text-[11px] tabular-nums">
        <span className="label text-[9px] tracking-[0.2em] text-zinc-600">PRODUCT&nbsp;SURFACE</span>
        <span className="text-cyan-300">PrometheonOS.submit</span>
        <span className="text-zinc-500">(signedTx)</span>
        <span className="text-zinc-600">→</span>
        <span className={landed ? "text-emerald-300" : "text-amber-300"}>{receipt}</span>
        <span className="ml-auto hidden text-[10px] text-zinc-600 sm:inline">
          headless infrastructure — the console above is the operator&apos;s view, not how it&apos;s used
        </span>
      </div>
    </footer>
  );
}

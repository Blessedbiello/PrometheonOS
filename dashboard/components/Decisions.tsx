"use client";

import { useEffect, useRef } from "react";
import { confidenceBars, formatMs } from "@/lib/format";
import type { DecisionRow, DecisionType } from "@/lib/types";

const TYPE_TONE: Record<DecisionType, string> = {
  tip: "text-amber-300",
  timing: "text-cyan-300",
  retry: "text-rose-300",
};

const norm = (s: string) => s.toLowerCase().replace(/[^a-z]/g, "");

/** A retry decision matches a highlighted failure class if its reasoning names that class. */
function matchesClass(d: DecisionRow, failureClass: string | null | undefined): boolean {
  if (!failureClass || d.decision_type !== "retry") return false;
  return norm(d.reasoning).includes(norm(failureClass));
}

export function Decisions({
  decisions,
  highlight,
}: {
  decisions: DecisionRow[];
  /** A failure class hovered on the rail — its retry decision gets spotlighted + scrolled into view. */
  highlight?: string | null;
}) {
  const hitRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (highlight && hitRef.current) {
      hitRef.current.scrollIntoView({ behavior: "smooth", block: "nearest" });
    }
  }, [highlight]);

  let firstHitAssigned = false;

  return (
    <section className="rounded-sm border border-zinc-900 bg-zinc-950/60 p-4">
      <h2 className="label text-[11px] tracking-[0.2em] text-zinc-300">AI Decision Timeline</h2>
      <div className="mt-3 max-h-[28rem] space-y-3 overflow-y-auto pr-1">
        {decisions.length === 0 ? (
          <p className="text-[11px] text-zinc-500">no decisions yet…</p>
        ) : (
          decisions.map((d, i) => {
            const hit = matchesClass(d, highlight);
            const isFirstHit = hit && !firstHitAssigned;
            if (isFirstHit) firstHitAssigned = true;
            return (
              <article
                key={`${d.ts}-${i}`}
                ref={isFirstHit ? hitRef : undefined}
                className={
                  "rounded-md border p-3 transition-all duration-300 " +
                  (hit
                    ? "border-amber-400/60 bg-amber-400/[0.06] shadow-[0_0_16px_-4px_rgba(251,191,36,0.5)]"
                    : "border-zinc-800/80 bg-zinc-900/50")
                }
              >
                <header className="mono flex items-baseline justify-between text-[10px] text-zinc-500">
                  <span>
                    <span className={`mr-2 uppercase ${TYPE_TONE[d.decision_type]}`}>
                      {d.decision_type}
                    </span>
                    <span className="text-zinc-600">{new Date(d.ts).toISOString().slice(11, 19)}</span>
                  </span>
                  <span title="confidence" className="text-zinc-400">
                    {confidenceBars(d.confidence)} {Math.round(d.confidence * 100)}%
                  </span>
                </header>
                <p className="mono mt-1 text-[12px] text-zinc-100">{d.action}</p>
                <p className="mt-1 text-[11px] leading-relaxed text-zinc-400">{d.reasoning}</p>
                {(d.before || d.after) && (
                  <pre className="mono mt-2 overflow-x-auto rounded bg-zinc-950/80 p-2 text-[10px] text-zinc-500">
                    {JSON.stringify({ before: d.before, after: d.after }, null, 0)}
                  </pre>
                )}
                <footer className="mono mt-1 text-[10px] text-zinc-600">
                  {d.provider} · {formatMs(d.latency_ms)}
                </footer>
              </article>
            );
          })
        )}
      </div>
    </section>
  );
}

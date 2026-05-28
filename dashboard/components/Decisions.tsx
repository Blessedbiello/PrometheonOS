import { confidenceBars, formatMs } from "@/lib/format";
import type { DecisionRow, DecisionType } from "@/lib/types";

const TYPE_TONE: Record<DecisionType, string> = {
  tip: "text-amber-300",
  timing: "text-cyan-300",
  retry: "text-rose-300",
};

export function Decisions({ decisions }: { decisions: DecisionRow[] }) {
  return (
    <section className="rounded-md border border-zinc-800 bg-zinc-950/60 p-4">
      <h2 className="text-xs font-semibold uppercase tracking-wider text-zinc-300">
        AI Decision Timeline
      </h2>
      <div className="mt-3 max-h-[28rem] space-y-3 overflow-y-auto pr-1">
        {decisions.length === 0 ? (
          <p className="text-[11px] text-zinc-500">no decisions yet…</p>
        ) : (
          decisions.map((d, i) => (
            <article
              key={`${d.ts}-${i}`}
              className="rounded-md border border-zinc-800/80 bg-zinc-900/50 p-3"
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
          ))
        )}
      </div>
    </section>
  );
}

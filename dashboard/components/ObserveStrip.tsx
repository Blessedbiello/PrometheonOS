"use client";

import type { HealthSnapshot } from "@/lib/types";

/** Log-scale position (%) of a lamport value on the [1k, 1M] tip axis. */
function logPos(lamports: number): number {
  const lo = Math.log10(1_000);
  const hi = Math.log10(1_000_000);
  const v = Math.log10(Math.max(1_000, Math.min(1_000_000, lamports)));
  return ((v - lo) / (hi - lo)) * 100;
}

function k(n: number): string {
  return n >= 1_000 ? `${Math.round(n / 1_000)}k` : `${n}`;
}

/**
 * The OBSERVE strip — the live inputs the AI reads, flowing DOWN into the rail. The star is the Jito
 * tip-floor distribution: P50 sits at the noise floor, so the agent anchors on the competitive P75–P95
 * band (highlighted). The deterministic floor clamps to 200k inside that band.
 */
export function ObserveStrip({
  health,
  nextJitoInSlots,
}: {
  health: HealthSnapshot;
  nextJitoInSlots: number | null;
}) {
  const p50 = health.tip_floor_lamports || 0;
  const p75 = health.tip_floor_p75_lamports ?? null;
  const p95 = health.tip_floor_p95_lamports ?? null;
  const FLOOR = 200_000; // the competitive deterministic floor

  return (
    <div className="flex items-stretch gap-4 border-b border-zinc-900 px-5 py-2 text-[10px]">
      <span className="label flex items-center text-[9px] tracking-[0.25em] text-zinc-600">OBSERVE</span>

      {/* congestion + landing probability, as thin readouts (not gauges) */}
      <div className="mono flex items-center gap-4 tabular-nums text-zinc-500">
        <span>
          congestion <span className="text-cyan-300">{health.congestion_score.toFixed(2)}</span>
        </span>
        <span>
          land-p <span className="text-emerald-300">{Math.round(health.bundle_landing_probability * 100)}%</span>
        </span>
        {nextJitoInSlots != null && (
          <span>
            next-jito <span className="text-amber-300">{nextJitoInSlots} slots</span>
          </span>
        )}
      </div>

      {/* the tip-floor distribution gauge */}
      <div className="relative ml-auto flex min-w-[280px] max-w-[420px] flex-1 items-center">
        <span className="label mr-3 text-[9px] tracking-[0.2em] text-zinc-600">TIP FLOOR</span>
        <div className="relative h-3 flex-1">
          {/* axis */}
          <div className="absolute top-1/2 h-px w-full -translate-y-1/2 bg-zinc-800" />
          {/* competitive P75–P95 band */}
          {p75 != null && p95 != null && (
            <div
              className="absolute top-1/2 h-1.5 -translate-y-1/2 rounded-sm bg-cyan-500/20"
              style={{ left: `${logPos(p75)}%`, width: `${logPos(p95) - logPos(p75)}%` }}
            />
          )}
          {/* the deterministic competitive floor marker */}
          <div
            className="absolute top-1/2 h-3.5 w-px -translate-y-1/2 bg-emerald-400/80"
            style={{ left: `${logPos(FLOOR)}%` }}
            title={`competitive floor ${FLOOR.toLocaleString()} lamports`}
          />
          {/* percentile ticks — labels staggered above/below so they never collide */}
          {[
            { v: p50, c: "bg-zinc-500", l: "P50", below: false },
            { v: p75, c: "bg-cyan-400", l: "P75", below: true },
            { v: p95, c: "bg-cyan-300", l: "P95", below: false },
          ].map(
            (t, i) =>
              t.v != null && (
                <div key={i} className="absolute top-0 -translate-x-1/2" style={{ left: `${logPos(t.v)}%` }}>
                  <div className={`mx-auto h-3 w-[2px] ${t.c}`} />
                  <span
                    className={
                      "mono absolute left-1/2 -translate-x-1/2 whitespace-nowrap text-[8px] tabular-nums text-zinc-500 " +
                      (t.below ? "top-3.5" : "-top-3")
                    }
                  >
                    {t.l}·{k(t.v)}
                  </span>
                </div>
              ),
          )}
        </div>
      </div>
    </div>
  );
}

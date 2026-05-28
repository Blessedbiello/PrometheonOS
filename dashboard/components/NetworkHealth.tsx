import { formatMs, formatPct } from "@/lib/format";
import type { HealthSnapshot } from "@/lib/types";

function Gauge({ label, value, tone = "cyan" }: { label: string; value: number; tone?: string }) {
  const pct = Math.round(value * 100);
  const colorBar =
    tone === "red"
      ? "from-rose-500 to-rose-400"
      : tone === "amber"
        ? "from-amber-500 to-amber-300"
        : tone === "emerald"
          ? "from-emerald-500 to-emerald-300"
          : "from-cyan-500 to-cyan-300";
  return (
    <div>
      <div className="flex items-baseline justify-between">
        <span className="text-[11px] uppercase tracking-wider text-zinc-400">{label}</span>
        <span className="mono text-sm text-zinc-100">{pct}%</span>
      </div>
      <div className="mt-1 h-1.5 w-full overflow-hidden rounded bg-zinc-800">
        <div
          className={`h-full bg-gradient-to-r ${colorBar}`}
          style={{ width: `${Math.max(0, Math.min(100, pct))}%` }}
        />
      </div>
    </div>
  );
}

export function NetworkHealth({ health }: { health: HealthSnapshot }) {
  const congestionTone =
    health.congestion_score >= 0.7
      ? "red"
      : health.congestion_score >= 0.4
        ? "amber"
        : "emerald";
  return (
    <section className="rounded-md border border-zinc-800 bg-zinc-950/60 p-4">
      <h2 className="text-xs font-semibold uppercase tracking-wider text-zinc-300">
        Network Health
      </h2>
      <div className="mt-4 space-y-3">
        <Gauge label="congestion" value={health.congestion_score} tone={congestionTone} />
        <Gauge label="slot stability" value={health.slot_stability_score} tone="emerald" />
        <Gauge label="landing probability" value={health.bundle_landing_probability} />
        <Gauge label="retry success" value={health.retry_success_rate} />
      </div>
      <div className="mt-4 grid grid-cols-2 gap-2 border-t border-zinc-800 pt-3 text-[11px]">
        <Metric label="avg p→c" value={formatMs(health.processed_to_confirmed_delta_ms)} />
        <Metric label="avg confirm" value={formatMs(health.avg_confirmed_latency_ms)} />
        <Metric label="tip floor" value={`${(health.tip_floor_lamports / 1000).toFixed(1)}k`} />
        <Metric
          label="tip efficiency"
          value={
            health.tip_efficiency_ratio === 0
              ? "—"
              : `${(health.tip_efficiency_ratio * 1_000_000).toFixed(2)} /µSOL`
          }
        />
      </div>
    </section>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between">
      <span className="text-zinc-500">{label}</span>
      <span className="mono text-zinc-100">{value}</span>
    </div>
  );
}

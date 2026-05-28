import { formatLamports, formatMs, formatSlot, stageColor, truncId } from "@/lib/format";
import type { BundleRow, LifecycleStage } from "@/lib/types";

const STAGES: LifecycleStage[] = ["submitted", "processed", "confirmed", "finalized"];

const COLOR_BG: Record<string, string> = {
  green: "bg-emerald-500/80",
  blue: "bg-cyan-400/80",
  yellow: "bg-amber-400/80",
  gray: "bg-zinc-600/80",
  red: "bg-rose-500/80",
};

function StageBar({ bundle }: { bundle: BundleRow }) {
  const isFailure = ["failed", "expired", "dropped"].includes(bundle.stage);
  const reachedIdx = STAGES.indexOf(bundle.stage);
  return (
    <div className="flex items-center gap-1.5">
      {STAGES.map((s, i) => {
        const reached = reachedIdx >= i && !isFailure;
        const isCurrent = s === bundle.stage;
        return (
          <span
            key={s}
            className={
              "h-1.5 w-10 rounded-sm " +
              (reached ? COLOR_BG[stageColor(s)] : "bg-zinc-800") +
              (isCurrent ? " ring-1 ring-cyan-400/60" : "")
            }
            title={s}
          />
        );
      })}
      {isFailure && (
        <span className="ml-1 text-[10px] font-medium uppercase tracking-wider text-rose-400">
          {bundle.stage}
          {bundle.injected ? " (injected)" : ""}
        </span>
      )}
    </div>
  );
}

export function Bundles({ bundles }: { bundles: BundleRow[] }) {
  return (
    <section className="rounded-md border border-zinc-800 bg-zinc-950/60 p-4">
      <h2 className="text-xs font-semibold uppercase tracking-wider text-zinc-300">
        Active Bundles & Lifecycle
      </h2>
      <div className="mt-3 overflow-x-auto">
        <table className="mono w-full text-[11px]">
          <thead className="text-zinc-500">
            <tr>
              <th className="text-left font-normal">bundle</th>
              <th className="text-right font-normal">tip</th>
              <th className="text-left font-normal pl-4">progression</th>
              <th className="text-right font-normal">slot</th>
              <th className="text-right font-normal">confirm</th>
              <th className="text-right font-normal">retry</th>
            </tr>
          </thead>
          <tbody>
            {bundles.length === 0 ? (
              <tr>
                <td colSpan={6} className="py-2 text-zinc-500">
                  no bundles in flight…
                </td>
              </tr>
            ) : (
              bundles
                .slice()
                .reverse()
                .map((b) => (
                  <tr key={b.bundle_id} className="border-t border-zinc-900">
                    <td className="py-1.5 text-zinc-300">{truncId(b.bundle_id, 8, 4)}</td>
                    <td className="py-1.5 text-right text-zinc-100">
                      {formatLamports(b.tip_lamports)}
                    </td>
                    <td className="py-1.5 pl-4">
                      <StageBar bundle={b} />
                    </td>
                    <td className="py-1.5 text-right text-zinc-400">
                      {b.slot ? formatSlot(b.slot) : "—"}
                    </td>
                    <td className="py-1.5 text-right text-zinc-300">
                      {formatMs(b.latencies.confirmed_ms)}
                    </td>
                    <td className="py-1.5 text-right text-zinc-400">
                      {b.retry_attempt > 0 ? `${b.retry_attempt}/3` : "—"}
                    </td>
                  </tr>
                ))
            )}
          </tbody>
        </table>
      </div>
    </section>
  );
}

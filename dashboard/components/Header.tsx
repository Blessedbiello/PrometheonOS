import { formatSlot } from "@/lib/format";

/** Connection/data state for the status indicator. `simulated` = serving the offline mock feed. */
export type FeedState = "live" | "simulated" | "off";

const INDICATOR: Record<FeedState, { dot: string; label: string; text: string }> = {
  live: { dot: "bg-emerald-400 animate-pulse", label: "live", text: "text-emerald-400" },
  simulated: { dot: "bg-amber-400", label: "simulated", text: "text-amber-400" },
  off: { dot: "bg-zinc-600", label: "off", text: "text-zinc-500" },
};

export function Header({
  network,
  slot,
  provider,
  feed,
}: {
  network: string;
  slot: number;
  provider: string;
  feed: FeedState;
}) {
  const ind = INDICATOR[feed];
  return (
    <header className="flex items-center justify-between border-b border-zinc-800 px-6 py-3">
      <div className="flex items-center gap-3">
        <span className="text-sm font-semibold tracking-wide text-cyan-400">PrometheonOS</span>
        <span className="text-xs text-zinc-500">/ execution-intelligence</span>
      </div>
      <div className="flex items-center gap-6 text-xs">
        <span className={"flex items-center gap-2 mono " + ind.text}>
          <span className={"h-2 w-2 rounded-full " + ind.dot} />
          {ind.label}
        </span>
        <span className="mono text-zinc-300">
          network <span className="text-zinc-100 font-medium">{network}</span>
        </span>
        <span className="mono text-zinc-300">
          slot <span className="text-zinc-100 font-medium">{formatSlot(slot)}</span>
        </span>
        <span className="mono text-zinc-300">
          provider <span className="text-zinc-100 font-medium">{provider}</span>
        </span>
      </div>
    </header>
  );
}

import { formatSlot } from "@/lib/format";

export function Header({
  network,
  slot,
  provider,
  live,
}: {
  network: string;
  slot: number;
  provider: string;
  live: boolean;
}) {
  return (
    <header className="flex items-center justify-between border-b border-zinc-800 px-6 py-3">
      <div className="flex items-center gap-3">
        <span className="text-sm font-semibold tracking-wide text-cyan-400">PrometheonOS</span>
        <span className="text-xs text-zinc-500">/ execution-intelligence</span>
      </div>
      <div className="flex items-center gap-6 text-xs">
        <span className="flex items-center gap-2 mono">
          <span
            className={
              "h-2 w-2 rounded-full " + (live ? "bg-emerald-400 animate-pulse" : "bg-zinc-600")
            }
          />
          {live ? "live" : "off"}
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

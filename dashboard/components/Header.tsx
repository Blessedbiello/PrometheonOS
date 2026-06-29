/** Connection/data state for the status indicator. */
export type FeedState = "live" | "simulated" | "proof-replay" | "off";

/** Which source the UI is requesting from the API. `proof` = replay of the committed mainnet run. */
export type SourceChoice = "live" | "mock" | "proof";

const INDICATOR: Record<FeedState, { dot: string; label: string; text: string }> = {
  live: { dot: "bg-emerald-400 animate-pulse", label: "LIVE", text: "text-emerald-400" },
  simulated: { dot: "bg-amber-400", label: "SIMULATED", text: "text-amber-400" },
  "proof-replay": { dot: "bg-cyan-400 animate-pulse", label: "PROOF-REPLAY", text: "text-cyan-400" },
  off: { dot: "bg-zinc-600", label: "OFF", text: "text-zinc-500" },
};

const SOURCES: { value: SourceChoice; label: string }[] = [
  { value: "proof", label: "PROOF" },
  { value: "live", label: "LIVE" },
  { value: "mock", label: "SIM" },
];

/** Slot odometer — tabular mono with the trailing 3 digits brightened so only they appear to tick. */
function SlotOdometer({ slot }: { slot: number }) {
  const s = slot > 0 ? slot.toLocaleString("en-US") : "—";
  const head = s.length > 4 ? s.slice(0, -4) : "";
  const tail = s.length > 4 ? s.slice(-4) : s;
  return (
    <span className="mono tabular-nums tracking-tight">
      <span className="text-zinc-500">{head}</span>
      <span className="text-cyan-300">{tail}</span>
    </span>
  );
}

export function Header({
  network,
  slot,
  provider,
  feed,
  source,
  onSource,
}: {
  network: string;
  slot: number;
  provider: string;
  feed: FeedState;
  source: SourceChoice;
  onSource: (s: SourceChoice) => void;
}) {
  const ind = INDICATOR[feed];
  return (
    <header className="flex items-center justify-between border-b border-zinc-900 px-5 py-2.5">
      <div className="flex items-baseline gap-3">
        <span className="label text-[13px] font-semibold text-cyan-400">PROMETHEON·OS</span>
        <span className="mono text-[10px] uppercase tracking-[0.2em] text-zinc-600">
          execution control plane
        </span>
      </div>

      <div className="flex items-center gap-5 text-[11px]">
        {/* Source toggle */}
        <div className="flex overflow-hidden rounded-sm border border-zinc-800">
          {SOURCES.map((s) => (
            <button
              key={s.value}
              onClick={() => onSource(s.value)}
              className={
                "mono px-2.5 py-1 text-[10px] tracking-wider transition-colors " +
                (source === s.value
                  ? "bg-zinc-800 text-zinc-100"
                  : "text-zinc-500 hover:text-zinc-300")
              }
            >
              {s.label}
            </button>
          ))}
        </div>

        <span className={"flex items-center gap-2 mono tracking-wider " + ind.text}>
          <span className={"h-2 w-2 rounded-full " + ind.dot} />
          {ind.label}
        </span>
        <span className="mono text-zinc-500">
          slot <SlotOdometer slot={slot} />
        </span>
        <span className="mono text-zinc-500">
          ai <span className="text-zinc-200">{provider}</span>
        </span>
        <span className="mono text-zinc-500">
          net <span className="text-zinc-200">{network}</span>
        </span>
      </div>
    </header>
  );
}

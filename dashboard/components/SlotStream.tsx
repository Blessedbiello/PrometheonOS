import { formatSlot } from "@/lib/format";
import type { SlotRow } from "@/lib/types";

const STATUS_COLOR: Record<string, string> = {
  processed: "text-yellow-400",
  confirmed: "text-cyan-300",
  finalized: "text-emerald-400",
  dead: "text-rose-400",
};

export function SlotStream({
  slots,
  nextJitoIn,
}: {
  slots: SlotRow[];
  nextJitoIn: number | null;
}) {
  return (
    <section className="rounded-md border border-zinc-800 bg-zinc-950/60 p-4">
      <div className="flex items-center justify-between">
        <h2 className="text-xs font-semibold uppercase tracking-wider text-zinc-300">
          Slot / Leader Stream
        </h2>
        <span className="text-[11px] text-zinc-400">
          next Jito leader: {nextJitoIn === null ? "—" : `+${nextJitoIn} slot${nextJitoIn === 1 ? "" : "s"}`}
        </span>
      </div>
      <div className="mt-3 max-h-[24rem] overflow-y-auto">
        <table className="mono w-full text-[11px]">
          <thead className="text-zinc-500">
            <tr>
              <th className="w-12 text-left font-normal">▸</th>
              <th className="text-left font-normal">slot</th>
              <th className="text-left font-normal">leader</th>
              <th className="text-left font-normal">status</th>
            </tr>
          </thead>
          <tbody>
            {slots.length === 0 ? (
              <tr>
                <td colSpan={4} className="py-2 text-zinc-500">
                  waiting for slots…
                </td>
              </tr>
            ) : (
              slots.map((s) => (
                <tr key={`${s.slot}-${s.status}`} className="border-t border-zinc-900">
                  <td className="py-1 text-zinc-600">▸</td>
                  <td className="py-1 text-zinc-200">{formatSlot(s.slot)}</td>
                  <td className="py-1 text-zinc-400">
                    {s.leader ? s.leader.slice(0, 10) + "…" : "—"}{" "}
                    {s.jito && <span className="ml-1 text-emerald-400">Jito✓</span>}
                  </td>
                  <td className={"py-1 " + (STATUS_COLOR[s.status] ?? "text-zinc-300")}>
                    {s.status}
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

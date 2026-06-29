"use client";

import { motion } from "framer-motion";
import { lamportsToSol, truncId } from "@/lib/format";
import type { BundleRow, DecisionRow } from "@/lib/types";

/** The four lifecycle stations, left → right across the rail. */
const STATIONS = ["SUBMITTED", "PROCESSED", "CONFIRMED", "FINALIZED"] as const;
const STAGE_INDEX: Record<string, number> = {
  submitted: 0,
  processed: 1,
  confirmed: 2,
  finalized: 3,
  // failures sit where they stopped (these injected faults are rejected at submit)
  failed: 0,
  expired: 0,
  dropped: 0,
};
// left % for each station centre, with padding for the labels/pills.
const STATION_X = [9, 37, 64, 90] as const;
/** Left-% of a station index, clamped (avoids undefined under noUncheckedIndexedAccess). */
const stationLeft = (i: number): number => STATION_X[Math.max(0, Math.min(3, i))] ?? 90;

function isFailed(b: BundleRow): boolean {
  return b.failure_class != null || ["failed", "expired", "dropped"].includes(b.stage);
}
function isLanded(b: BundleRow): boolean {
  return b.stage === "confirmed" || b.stage === "finalized";
}
function tipSol(lamports: number): string {
  return lamportsToSol(lamports, lamports >= 1_000_000 ? 4 : 6).replace(" SOL", "◎");
}

interface Chain {
  base: string;
  attempts: BundleRow[]; // ordered by attempt
}

function toChains(bundles: BundleRow[]): Chain[] {
  const by = new Map<string, BundleRow[]>();
  for (const b of bundles) {
    const key = b.base_id ?? b.bundle_id;
    (by.get(key) ?? by.set(key, []).get(key)!).push(b);
  }
  const chains = [...by.entries()].map(([base, attempts]) => ({
    base,
    attempts: attempts.slice().sort((a, b) => (a.attempt ?? 1) - (b.attempt ?? 1)),
  }));
  // Stable order by base id so rows don't jump around between polls.
  chains.sort((a, b) => a.base.localeCompare(b.base));
  return chains;
}

/** Short human label for the AI's chosen lever on a retry decision. */
function leverLabel(d: DecisionRow | undefined): string {
  if (!d) return "";
  const refresh = (d.after as Record<string, unknown> | null)?.refresh_blockhash === true;
  return refresh ? "↻ refresh blockhash · keep tip" : "↑ raise tip · keep blockhash";
}

function Token({ b, explorerBase }: { b: BundleRow; explorerBase: string }) {
  const failed = isFailed(b);
  const landed = isLanded(b);
  const x = stationLeft(STAGE_INDEX[b.stage] ?? 0);
  const tone = failed
    ? "border-rose-500/70 bg-rose-500/15 text-rose-200 shadow-[0_0_12px_-2px_rgba(244,63,94,0.6)]"
    : landed
      ? "border-emerald-500/70 bg-emerald-500/15 text-emerald-200 shadow-[0_0_12px_-2px_rgba(16,185,129,0.6)]"
      : "border-cyan-500/60 bg-cyan-500/10 text-cyan-200 shadow-[0_0_12px_-2px_rgba(34,211,238,0.5)]";
  const haloAmber = b.injected ? " ring-1 ring-amber-400/40" : "";

  const body = (
    <motion.span
      layout
      initial={{ opacity: 0, scale: 0.8 }}
      animate={{ opacity: 1, scale: 1 }}
      transition={{ type: "spring", stiffness: 320, damping: 26 }}
      className={
        "mono inline-flex items-center gap-1.5 whitespace-nowrap rounded-sm border px-1.5 py-0.5 text-[10px] tabular-nums transition-[background-color,border-color,box-shadow,color] duration-500 " +
        tone +
        haloAmber
      }
    >
      <span className="opacity-70">{truncId(b.bundle_id, 4, 3)}</span>
      <span className="font-semibold">{tipSol(b.tip_lamports)}</span>
      {b.stage === "finalized" && b.slot ? (
        <span className="text-emerald-300/90">· {b.slot.toLocaleString("en-US")}</span>
      ) : null}
    </motion.span>
  );

  return (
    <div
      className="absolute top-1/2 -translate-x-1/2 -translate-y-1/2 transition-[left] duration-700 ease-out"
      style={{ left: `${x}%` }}
      title={b.bundle_id}
    >
      {b.stage === "finalized" && b.slot ? (
        <a href={`${explorerBase}/block/${b.slot}`} target="_blank" rel="noreferrer">
          {body}
        </a>
      ) : (
        body
      )}
    </div>
  );
}

/** One logical-bundle row. A recovery chain shows the failed attempt, the AI lever, and the landing. */
function Row({
  chain,
  retry,
  explorerBase,
}: {
  chain: Chain;
  retry: DecisionRow | undefined;
  explorerBase: string;
}) {
  const failedAttempt = chain.attempts.find(isFailed);
  const recovered = chain.attempts.length > 1 && chain.attempts.some(isLanded) && !!failedAttempt;

  return (
    <div className="relative h-8 border-b border-zinc-900/70">
      {/* connecting hairline behind the tokens */}
      <div
        className="absolute top-1/2 h-px -translate-y-1/2 bg-zinc-800"
        style={{ left: `${stationLeft(0)}%`, right: `${100 - stationLeft(3)}%` }}
      />
      {recovered && failedAttempt && (
        <>
          {/* recovery underline: rose at the fault, fading to emerald at the landing */}
          <div
            className="absolute bottom-1 h-px bg-gradient-to-r from-rose-500/70 via-amber-400/50 to-emerald-500/70"
            style={{ left: `${stationLeft(0)}%`, right: `${100 - stationLeft(3)}%` }}
          />
          {/* the AI lever the operator pulled, inline at mid-rail */}
          <motion.span
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.4 }}
            className="mono absolute left-1/2 top-0 -translate-x-1/2 text-[9px] uppercase tracking-wider text-amber-300/90"
          >
            {failedAttempt.failure_class}
            {failedAttempt.failure_confidence != null
              ? ` ${(failedAttempt.failure_confidence * 100).toFixed(0)}%`
              : ""}{" "}
            <span className="text-zinc-600">→</span> <span className="text-amber-200">{leverLabel(retry)}</span>
          </motion.span>
        </>
      )}
      {chain.attempts.map((b) => (
        <Token key={b.bundle_id} b={b} explorerBase={explorerBase} />
      ))}
      <span className="mono absolute -left-0.5 top-1/2 -translate-x-full -translate-y-1/2 pr-2 text-[9px] text-zinc-600">
        {chain.base}
      </span>
    </div>
  );
}

export function ExecutionRail({
  bundles,
  decisions,
  explorerBase = "https://explorer.solana.com",
}: {
  bundles: BundleRow[];
  decisions: DecisionRow[];
  explorerBase?: string;
}) {
  const chains = toChains(bundles);
  const retries = decisions.filter((d) => d.decision_type === "retry");
  const retryByClass = (cls: string | null | undefined): DecisionRow | undefined => {
    if (!cls) return undefined;
    const want = cls.toLowerCase().replace(/[^a-z]/g, "");
    return retries.find((d) => d.reasoning.toLowerCase().replace(/[^a-z]/g, "").includes(want));
  };

  const landed = bundles.filter(isLanded).length;
  const failedNow = bundles.filter(isFailed);
  const recoveries = chains.filter(
    (c) => c.attempts.some(isFailed) && c.attempts.some(isLanded),
  ).length;
  const activeRetry = failedNow.length > 0;
  const lastRetry = retries[retries.length - 1];
  const provider = decisions[decisions.length - 1]?.provider ?? "openai";

  return (
    <section className="relative overflow-hidden rounded-sm border border-zinc-900 bg-[#0b0b0c] px-4 pb-3 pt-2">
      {/* engraved graticule */}
      <div className="graticule pointer-events-none absolute inset-0 opacity-60" />

      {/* title + KPIs */}
      <div className="relative mb-1 flex items-baseline justify-between">
        <h2 className="label text-[11px] font-semibold tracking-[0.25em] text-zinc-300">EXECUTION RAIL</h2>
        <div className="mono flex items-center gap-4 text-[10px] tabular-nums text-zinc-500">
          <span>
            landed <span className="text-emerald-300">{landed}</span>/<span className="text-zinc-300">{bundles.length}</span>
          </span>
          <span>
            ai-recovered <span className="text-cyan-300">{recoveries}</span>
          </span>
        </div>
      </div>

      {/* station header */}
      <div className="relative mb-1 h-4">
        {STATIONS.map((s, i) => (
          <span
            key={s}
            className="label absolute -translate-x-1/2 text-[9px] tracking-[0.2em] text-zinc-600"
            style={{ left: `${stationLeft(i)}%` }}
          >
            {s}
          </span>
        ))}
      </div>

      {/* station gridlines */}
      <div className="relative">
        {STATION_X.map((x, i) => (
          <div
            key={i}
            className="pointer-events-none absolute bottom-0 top-0 w-px bg-zinc-800/70"
            style={{ left: `${x}%` }}
          />
        ))}

        {/* rows */}
        <div className="relative pl-7">
          {chains.length === 0 ? (
            <p className="mono py-6 text-center text-[11px] text-zinc-600">awaiting bundles…</p>
          ) : (
            chains.map((c) => (
              <Row key={c.base} chain={c} retry={retryByClass(c.attempts.find(isFailed)?.failure_class)} explorerBase={explorerBase} />
            ))
          )}
        </div>
      </div>

      {/* AI OPERATOR node */}
      <div
        className={
          "relative mt-2 flex items-center gap-3 rounded-sm border px-3 py-1.5 transition-colors duration-500 " +
          (activeRetry
            ? "border-amber-400/50 bg-amber-400/5"
            : "border-zinc-800 bg-zinc-950/40")
        }
      >
        <span
          className={
            "h-2.5 w-2.5 shrink-0 rounded-full " +
            (activeRetry ? "animate-pulse bg-amber-400" : "bg-cyan-500/70")
          }
        />
        <span className="label text-[10px] tracking-[0.2em] text-cyan-300">AI OPERATOR</span>
        <span className="mono text-[10px] text-zinc-500">{provider} · gpt-oss-120b via NATS</span>
        <span className="mono ml-auto text-[10px] tabular-nums text-zinc-400">
          {activeRetry && lastRetry ? (
            <>
              <span className="text-amber-300">{lastRetry.action}</span>
              {lastRetry.before || lastRetry.after ? (
                <span className="ml-2 text-zinc-500">
                  {JSON.stringify(lastRetry.after)}
                </span>
              ) : null}
            </>
          ) : (
            <span className="text-zinc-600">monitoring · clamps tip to competitive floor · forces refresh on expiry</span>
          )}
        </span>
      </div>
    </section>
  );
}

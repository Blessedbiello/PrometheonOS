"use client";

import { useEffect, useState } from "react";
import { Bundles } from "@/components/Bundles";
import { Decisions } from "@/components/Decisions";
import { ExecutionRail } from "@/components/ExecutionRail";
import { Header, type FeedState, type SourceChoice } from "@/components/Header";
import { NetworkHealth } from "@/components/NetworkHealth";
import { ObserveStrip } from "@/components/ObserveStrip";
import { ReceiptStrip } from "@/components/ReceiptStrip";
import { SlotStream } from "@/components/SlotStream";
import type { DashboardSnapshot } from "@/lib/types";

const POLL_MS = 1_000;

function feedFor(source: DashboardSnapshot["source"] | undefined): FeedState {
  if (source === "live") return "live";
  if (source === "proof") return "proof-replay";
  if (source === "mock") return "simulated";
  return "off";
}

export default function Page() {
  const [snap, setSnap] = useState<DashboardSnapshot | null>(null);
  const [feed, setFeed] = useState<FeedState>("off");
  // Default to the committed-mainnet proof replay so the recovery hero plays on load — honestly badged.
  const [source, setSource] = useState<SourceChoice>("proof");
  // Hovering a recovery row on the rail spotlights its decision in the timeline (and vice-versa).
  const [highlight, setHighlight] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;

    // Optional ?t=<ms> scrubs the proof replay to a fixed frame (demo/screenshots); else it loops live.
    const scrub =
      typeof window !== "undefined" ? new URLSearchParams(window.location.search).get("t") : null;

    const poll = async () => {
      try {
        const url = `/api/telemetry?source=${source}${scrub ? `&t=${scrub}` : ""}`;
        const res = await fetch(url, { cache: "no-store" });
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const data = (await res.json()) as DashboardSnapshot;
        if (!cancelled) {
          setSnap(data);
          setFeed(feedFor(data.source));
        }
      } catch {
        if (!cancelled) setFeed("off");
      } finally {
        if (!cancelled) timer = setTimeout(poll, POLL_MS);
      }
    };
    poll();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [source]);

  return (
    <main className="flex min-h-screen flex-col">
      <Header
        network={snap?.network ?? "—"}
        slot={snap?.current_slot ?? 0}
        provider={snap?.ai_provider ?? "—"}
        feed={feed}
        source={source}
        onSource={setSource}
      />

      {snap ? (
        <ObserveStrip health={snap.health} nextJitoInSlots={snap.next_jito_leader_in_slots} />
      ) : (
        <div className="h-10 border-b border-zinc-900" />
      )}

      <div className="flex-1 space-y-4 p-4">
        {/* HERO — the closed control loop made geometry */}
        {snap ? (
          <ExecutionRail bundles={snap.bundles} decisions={snap.decisions} onHighlight={setHighlight} />
        ) : (
          <Skeleton label="Execution Rail" tall />
        )}

        {/* Operator's decision drawer + a compact health readout */}
        <div className="grid grid-cols-12 gap-4">
          <div className="col-span-12 lg:col-span-7">
            {snap ? (
              <Decisions decisions={snap.decisions} highlight={highlight} />
            ) : (
              <Skeleton label="AI Decision Timeline" />
            )}
          </div>
          <div className="col-span-12 lg:col-span-5">
            {snap ? <NetworkHealth health={snap.health} /> : <Skeleton label="Network Health" />}
          </div>
        </div>

        {/* Raw instrumentation for an operator who wants the tape — demoted, not deleted */}
        {snap && (
          <details className="rounded-sm border border-zinc-900 bg-zinc-950/40">
            <summary className="label cursor-pointer px-4 py-2 text-[10px] tracking-[0.2em] text-zinc-500 hover:text-zinc-300">
              Instrumentation · raw tape
            </summary>
            <div className="grid grid-cols-12 gap-4 p-4 pt-0">
              <div className="col-span-12 lg:col-span-5">
                <SlotStream slots={snap.slots} nextJitoIn={snap.next_jito_leader_in_slots} />
              </div>
              <div className="col-span-12 lg:col-span-7">
                <Bundles bundles={snap.bundles} />
              </div>
            </div>
          </details>
        )}
      </div>

      <ReceiptStrip bundles={snap?.bundles ?? []} />
    </main>
  );
}

function Skeleton({ label, tall = false }: { label: string; tall?: boolean }) {
  return (
    <section className="rounded-sm border border-zinc-900 bg-zinc-950/50 p-4">
      <h2 className="label text-[11px] tracking-[0.2em] text-zinc-500">{label}</h2>
      <div className={"mt-3 animate-pulse rounded bg-zinc-900/50 " + (tall ? "h-80" : "h-24")} />
    </section>
  );
}

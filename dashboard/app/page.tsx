"use client";

import { useEffect, useState } from "react";
import { Bundles } from "@/components/Bundles";
import { Decisions } from "@/components/Decisions";
import { Header, type FeedState } from "@/components/Header";
import { NetworkHealth } from "@/components/NetworkHealth";
import { SlotStream } from "@/components/SlotStream";
import type { DashboardSnapshot } from "@/lib/types";

const POLL_MS = 1_000;

export default function Page() {
  const [snap, setSnap] = useState<DashboardSnapshot | null>(null);
  // The status indicator reflects the DATA SOURCE, not merely whether the fetch succeeded — a
  // successful fetch of the mock feed must read "simulated", never "live".
  const [feed, setFeed] = useState<FeedState>("off");

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const poll = async () => {
      try {
        const res = await fetch("/api/telemetry", { cache: "no-store" });
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const data = (await res.json()) as DashboardSnapshot;
        if (!cancelled) {
          setSnap(data);
          setFeed(data.source === "live" ? "live" : "simulated");
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
  }, []);

  return (
    <main className="min-h-screen">
      <Header
        network={snap?.network ?? "—"}
        slot={snap?.current_slot ?? 0}
        provider={snap?.ai_provider ?? "—"}
        feed={feed}
      />

      <div className="grid grid-cols-12 gap-4 p-4">
        <div className="col-span-12 lg:col-span-3">
          {snap ? (
            <NetworkHealth health={snap.health} />
          ) : (
            <SkeletonCard label="Network Health" />
          )}
        </div>

        <div className="col-span-12 lg:col-span-5">
          {snap ? (
            <SlotStream slots={snap.slots} nextJitoIn={snap.next_jito_leader_in_slots} />
          ) : (
            <SkeletonCard label="Slot / Leader Stream" />
          )}
        </div>

        <div className="col-span-12 lg:col-span-4 lg:row-span-2">
          {snap ? (
            <Decisions decisions={snap.decisions} />
          ) : (
            <SkeletonCard label="AI Decision Timeline" />
          )}
        </div>

        <div className="col-span-12 lg:col-span-8">
          {snap ? (
            <Bundles bundles={snap.bundles} />
          ) : (
            <SkeletonCard label="Active Bundles & Lifecycle" />
          )}
        </div>
      </div>
    </main>
  );
}

function SkeletonCard({ label }: { label: string }) {
  return (
    <section className="rounded-md border border-zinc-800 bg-zinc-950/60 p-4">
      <h2 className="text-xs font-semibold uppercase tracking-wider text-zinc-300">{label}</h2>
      <div className="mt-3 h-24 animate-pulse rounded bg-zinc-900/50" />
    </section>
  );
}

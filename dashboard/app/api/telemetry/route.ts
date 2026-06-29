import { NextResponse } from "next/server";
import { MockState } from "@/lib/mock";
import { ensureConnected, liveTelemetry } from "@/lib/live";
import { proofReplay, snapshotAtElapsed } from "@/lib/proof";

// `nats` is a Node TCP client and proof-replay reads the filesystem — pin to the Node runtime.
export const runtime = "nodejs";
export const dynamic = "force-dynamic";

// Default source when the request doesn't pin one. `auto` serves live telemetry when the bus is
// producing, else the mock simulation.
const DEFAULT_SOURCE = (process.env.TELEMETRY_SOURCE ?? "auto").toLowerCase();
const NATS_URL = process.env.NATS_URL ?? "nats://localhost:4222";

// One mock simulation per server process (the offline fallback / "simulated" demo).
const mock = new MockState(Math.floor(Math.random() * 1_000_000));

const json = (data: unknown) =>
  NextResponse.json(data, { headers: { "cache-control": "no-store" } });

export async function GET(request: Request) {
  const params = new URL(request.url).searchParams;
  // The UI's source toggle pins a source via ?source=; otherwise fall back to the env default.
  const requested = (params.get("source") ?? DEFAULT_SOURCE).toLowerCase();

  // proof: a deterministic replay of the committed mainnet run (real on-chain data + explorer links),
  // tagged source:"proof" → the UI renders the honest "proof-replay" badge. An optional ?t=<ms> scrubs
  // to a precise frame (used to park the demo on the recovery moment / for reproducible screenshots).
  if (requested === "proof") {
    const t = params.get("t");
    if (t != null && Number.isFinite(Number(t))) {
      return json(snapshotAtElapsed(Number(t)));
    }
    return json(proofReplay().snapshot());
  }

  if (requested !== "mock") {
    const connected = await ensureConnected(NATS_URL).catch(() => false);
    const live = liveTelemetry();
    if (connected && (requested === "live" || live.hasData())) {
      return json(live.snapshot()); // carries source:"live"
    }
  }
  // `mock.tick()` carries source:"mock" — the UI renders this as "simulated", never "live".
  return json(mock.tick());
}

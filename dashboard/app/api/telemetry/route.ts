import { NextResponse } from "next/server";
import { MockState } from "@/lib/mock";
import { ensureConnected, liveTelemetry } from "@/lib/live";

// `nats` is a Node TCP client — pin this route to the Node runtime.
export const runtime = "nodejs";
export const dynamic = "force-dynamic";

// auto: serve live telemetry when the bus is producing, else fall back to the mock simulation.
// live: always serve live (even if empty). mock: always serve the simulation.
const SOURCE = (process.env.TELEMETRY_SOURCE ?? "auto").toLowerCase();
const NATS_URL = process.env.NATS_URL ?? "nats://localhost:4222";

// One mock simulation per server process (used as the fallback / offline demo).
const mock = new MockState(Math.floor(Math.random() * 1_000_000));

export async function GET() {
  if (SOURCE !== "mock") {
    const connected = await ensureConnected(NATS_URL).catch(() => false);
    const live = liveTelemetry();
    if (connected && (SOURCE === "live" || live.hasData())) {
      return NextResponse.json(live.snapshot(), { headers: { "cache-control": "no-store" } });
    }
  }
  return NextResponse.json(mock.tick(), { headers: { "cache-control": "no-store" } });
}

import { NextResponse } from "next/server";
import { MockState } from "@/lib/mock";

// One singleton state per server process; ticks per request to advance the simulation.
const state = new MockState(Math.floor(Math.random() * 1_000_000));

export const dynamic = "force-dynamic";

export async function GET() {
  const snap = state.tick();
  return NextResponse.json(snap, {
    headers: { "cache-control": "no-store" },
  });
}

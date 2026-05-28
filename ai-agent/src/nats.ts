/**
 * NATS wiring: subscribe to `decision.request.*`, run the agent, reply with the assembled
 * `Decision`, and publish it to `decision.<type>` for the dashboard.
 *
 * `handleDecisionRequest` (the pure parse→decide→serialize step) is unit-tested; `runAgent` is the
 * thin connection/subscription loop exercised against a running NATS server.
 */
import { connect, type NatsConnection } from "nats";
import { z } from "zod";
import { decide } from "./agent.js";
import { decisionTypeSchema } from "./schema.js";
import type { LlmProvider } from "./providers/types.js";

const requestSchema = z.object({
  decisionType: decisionTypeSchema,
  context: z.record(z.string(), z.unknown()),
});

/** Parse a request payload, produce a decision, and return the serialized `Decision` JSON. */
export async function handleDecisionRequest(provider: LlmProvider, raw: string): Promise<string> {
  const req = requestSchema.parse(JSON.parse(raw));
  const decision = await decide(provider, req);
  return JSON.stringify(decision);
}

/** Connect to NATS and serve decision requests until the connection closes. */
export async function runAgent(provider: LlmProvider, natsUrl: string): Promise<NatsConnection> {
  const nc = await connect({ servers: natsUrl });
  const sub = nc.subscribe("decision.request.*");
  (async () => {
    for await (const msg of sub) {
      try {
        const out = await handleDecisionRequest(provider, msg.string());
        msg.respond(new TextEncoder().encode(out));
        // Also publish to decision.<type> for the dashboard timeline.
        const subject = `decision.${msg.subject.split(".").pop()}`;
        nc.publish(subject, new TextEncoder().encode(out));
      } catch (err) {
        const detail = err instanceof Error ? err.message : String(err);
        msg.respond(new TextEncoder().encode(JSON.stringify({ error: detail })));
      }
    }
  })();
  return nc;
}

# RFC 0001 — Inter-process bus: NATS for telemetry + AI decision request/reply

**Status:** accepted · **Phase:** 0 (foundational)

## Context
The Rust core (deterministic hot path) and the TS AI agent (asynchronous strategist) are separate
processes by design (clean AI/core separation, per the bounty). They need: (a) the core to publish
telemetry to the agent + dashboard, and (b) the core to ask the agent for a decision and receive a
structured answer.

## Decision
Use **NATS** as both the pub/sub telemetry bus and the request/reply transport for decisions.
- Telemetry: core publishes to `telemetry.*`; agent + dashboard subscribe.
- Decisions: core issues `request(decision.request.<type>, ctx)`; agent replies with a structured
  `Decision`; the agent also publishes the decision to `decision.<type>` for the dashboard.

## Rationale
NATS gives native request/reply *and* pub/sub in one lightweight single-binary broker, decoupling
the two languages cleanly. Alternatives (gRPC bidi, Redis streams) were heavier or weaker at
request/reply. Latency of the LLM is irrelevant to the hot path because the core only consults the
agent on non-microsecond-critical events and otherwise runs on the latest cached policy.

## Consequences
- One extra infra dependency (NATS in docker-compose) — acceptable.
- Messages are JSON validated against the shared schema (`contracts/`).
- Backpressure between core and agent is bounded by NATS; the hot path never blocks on a reply
  (decision requests are awaited only off the critical path / with timeouts + fallback policy).

# contracts/ — cross-language schema (single source of truth)

One definition drives every language. The Rust telemetry types derive `serde` + `schemars`;
`scripts/gen-contracts.sh` emits their JSON Schema here, and TypeScript types are generated from
those schemas. So the Rust core, the NATS/Postgres wire format, and the TypeScript agent + dashboard
all share **one** contract — and CI fails if it drifts.

```
contracts/
  json-schema/        # generated JSON Schema (from the Rust types) — checked in
    telemetry-event.schema.json   # the TelemetryEvent envelope (+ all nested types)
    decision.schema.json          # the AI Decision reasoning trace
  ts/                 # generated TS types (from JSON Schema) — checked in
    telemetry-event.d.ts
    decision.d.ts
```

## Regenerate

```bash
./scripts/gen-contracts.sh          # Rust → JSON Schema → TS, in one step
# or just the Rust → JSON Schema half:
cargo run -p prometheon-telemetry --bin schema-gen
```

## Drift check (CI) — both halves are gated

```bash
# half 1 — Rust → JSON Schema (rust CI job)
cargo run -p prometheon-telemetry --bin schema-gen -- --check
# half 2 — JSON Schema → TS (ts CI job): regenerate contracts/ts and `git diff --exit-code`
```
The rust job regenerates the schemas in-memory and fails if `json-schema/` differs from the Rust
types; the ts job regenerates `ts/` from `json-schema/` and fails if it differs from the committed
files. So a change on either side that isn't reflected here breaks the build — the contract cannot
silently drift in either direction.

## Not a hand-edited boundary

Never edit `json-schema/` or `ts/` by hand — edit the Rust types and regenerate. The generated
`ts/` types are the canonical *wire* shapes; the dashboard additionally keeps a few
presentation-only fields (e.g. `processed_to_confirmed_delta_ms`, per-slot leader/jito flags)
layered on top.

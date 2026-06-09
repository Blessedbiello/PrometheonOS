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

## Drift check (CI)

```bash
cargo run -p prometheon-telemetry --bin schema-gen -- --check
```
Regenerates the schemas in-memory and fails if the checked-in files differ. This runs in CI, so a
change to a Rust contract type that isn't reflected here (or vice-versa) breaks the build.

## Not a hand-edited boundary

Never edit `json-schema/` or `ts/` by hand — edit the Rust types and regenerate. The generated
`ts/` types are the canonical *wire* shapes; the dashboard additionally keeps a few
presentation-only fields (e.g. `processed_to_confirmed_delta_ms`, per-slot leader/jito flags)
layered on top.

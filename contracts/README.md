# contracts/ — cross-language schema (single source of truth)

Rust types in `prometheon-types` derive `serde` + `schemars`. A generator (wired in **Phase 4**)
emits JSON Schema here; TypeScript types for the AI agent and dashboard are generated from these
schemas and validated at runtime with zod. CI fails on schema drift.

```
contracts/
  json-schema/        # generated JSON Schema (from Rust) — checked in
  ts/                 # generated TS types (from JSON Schema) — checked in
```

Generation is not a hand-edited boundary: edit the Rust types, regenerate, commit the result.

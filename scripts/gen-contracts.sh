#!/usr/bin/env bash
# Regenerate the cross-language contract from the Rust types (single source of truth):
#   Rust (schemars) ──> contracts/json-schema/*.json ──> contracts/ts/*.d.ts
#
# Run from anywhere; CI verifies the Rust→JSON-Schema step has not drifted via
# `cargo run -p prometheon-telemetry --bin schema-gen -- --check`.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "1/2  Rust types  ->  contracts/json-schema/"
cargo run -q -p prometheon-telemetry --bin schema-gen

echo "2/2  JSON Schema ->  contracts/ts/"
mkdir -p contracts/ts
for f in contracts/json-schema/*.schema.json; do
  base="$(basename "$f" .schema.json)"
  npx --yes json-schema-to-typescript@15 -i "$f" -o "contracts/ts/${base}.d.ts"
  echo "  wrote contracts/ts/${base}.d.ts"
done

echo "done. Review + commit contracts/."

#!/usr/bin/env bash
# Mainnet proof run: submit N Jito bundles (incl. injected failures), persist the lifecycle
# telemetry, then export the lifecycle log.
#
#   NETWORK=mainnet ./scripts/run-proof.sh [COUNT] [INJECT]
#
# Prerequisites:
#   - infra up:            docker compose -f infra/docker-compose.yml up -d   # NATS + Postgres
#   - .env filled:         SolInfra RPC/Yellowstone, JITO_*_MAINNET, WALLET_KEYPAIR_PATH_MAINNET,
#                          DATABASE_URL (the export reads the persisted telemetry from Postgres)
#   - mainnet wallet FUNDED (the proof submits real bundles + tips)
#   - optionally the AI agent running with LLM_PROVIDER=anthropic for real decision traces
#
# The proof binary owns the single Yellowstone stream and emits Bundle/Lifecycle/Failure telemetry
# itself — do NOT also run the engine (`prometheon`) against the same SolInfra plan (1 stream).
#
# Without funds, run the (free) dry-run instead to validate everything but broadcast:
#   NETWORK=mainnet cargo run -p prometheon-core --bin proof -- --count "$COUNT"
set -euo pipefail
cd "$(dirname "$0")/.."

COUNT="${1:-12}"
INJECT="${2:-low-tip:1,stale-blockhash:1}"   # guarantees the bounty's ≥2 classified failure cases
: "${NETWORK:=mainnet}"
export NETWORK

echo "── PrometheonOS proof run (network=$NETWORK, count=$COUNT, inject=$INJECT) ───────────"

# Submit + stream-track + persist telemetry (one shared stream; NATS + Postgres sinks).
echo "submitting $COUNT bundles…"
LOG_LEVEL="${LOG_LEVEL:-info}" \
  cargo run -q -p prometheon-core --bin proof -- --live --count "$COUNT" --inject "$INJECT"

# Let the last bundles reach finalized, then export the lifecycle log from the persisted telemetry.
sleep 20
echo "exporting lifecycle log…"
EXPLORER_BASE="${EXPLORER_BASE:-https://explorer.solana.com}" \
  cargo run -q -p prometheon-telemetry --bin export-log

echo "── done. See logs/lifecycle-log.{json,md} ──────────────────────────────────────"

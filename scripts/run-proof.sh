#!/usr/bin/env bash
# Mainnet proof run: submit N Jito bundles (incl. injected failures), then export the lifecycle log.
#
#   NETWORK=mainnet ./scripts/run-proof.sh [COUNT]
#
# Prerequisites:
#   - infra up:            docker compose -f infra/docker-compose.yml up -d
#   - .env filled:         SolInfra RPC/Yellowstone, JITO_*_MAINNET, WALLET_KEYPAIR_PATH_MAINNET
#   - mainnet wallet FUNDED (the proof submits real bundles + tips)
#
# Without funds, run the (free) dry-run instead to validate everything but broadcast:
#   NETWORK=mainnet cargo run -p prometheon-core --bin proof -- --count "$COUNT"
set -euo pipefail
cd "$(dirname "$0")/.."

COUNT="${1:-12}"
: "${NETWORK:=mainnet}"
export NETWORK

echo "── PrometheonOS proof run (network=$NETWORK, count=$COUNT) ───────────────────────"

# 1) Engine in the background: streams slots + persists telemetry (NATS + Postgres) + /metrics.
echo "starting engine (telemetry sinks)…"
LOG_LEVEL="${LOG_LEVEL:-info}" cargo run -q -p prometheon-core --bin prometheon &
ENGINE_PID=$!
trap 'kill "$ENGINE_PID" 2>/dev/null || true' EXIT
sleep 5

# 2) Submit bundles live (the proof binary tracks each via the stream and emits telemetry).
echo "submitting $COUNT bundles…"
cargo run -q -p prometheon-core --bin proof -- --live --count "$COUNT"

# 3) Let the last bundles reach finalized, then export the lifecycle log.
sleep 20
echo "exporting lifecycle log…"
EXPLORER_BASE="${EXPLORER_BASE:-https://explorer.solana.com}" \
  cargo run -q -p prometheon-telemetry --bin export-log

echo "── done. See logs/lifecycle-log.{json,md} ──────────────────────────────────────"

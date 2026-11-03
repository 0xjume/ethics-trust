#!/usr/bin/env bash
# Deploy keyless-trust to m2.<parent>.testnet.
# Parent default: keyless-trust-dev-54ef71.testnet
#
# Requires (not in this environment — no secrets here):
#   NEAR_ACCOUNT_ID   throwaway parent on testnet
#   NEAR_PRIVATE_KEY  ed25519:... for that account
#   EXECUTE_DEPLOY=1
#
# Testnet only. Refuses mainnet. Does not print the private key.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [[ "${NEAR_NETWORK:-testnet}" != "testnet" ]]; then
  echo "refusing to deploy: NEAR_NETWORK must be testnet" >&2
  exit 1
fi

if [[ -z "${NEAR_ACCOUNT_ID:-}" || -z "${NEAR_PRIVATE_KEY:-}" ]]; then
  echo "NEAR_ACCOUNT_ID and NEAR_PRIVATE_KEY are not set." >&2
  echo "Build the wasm, then re-run with those env vars and EXECUTE_DEPLOY=1:" >&2
  echo "  cargo build -p keyless-trust --target wasm32-unknown-unknown --release" >&2
  echo "  EXECUTE_DEPLOY=1 pnpm --filter @keyless-trust/relayer deploy-testnet" >&2
  echo "Target: \${DEPLOY_SUBACCOUNT:-m2}.\${NEAR_ACCOUNT_ID:-keyless-trust-dev-54ef71.testnet}" >&2
  exit 2
fi

cargo build -p keyless-trust --target wasm32-unknown-unknown --release
export EXECUTE_DEPLOY="${EXECUTE_DEPLOY:-1}"
exec pnpm --filter @keyless-trust/relayer deploy-testnet

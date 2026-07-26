#!/usr/bin/env bash
#
# testnet-smoke.sh — read-only smoke test against a Market contract already
# deployed on Stellar testnet.
#
# This performs a single simulated (never signed/submitted) invocation of a
# harmless read-only getter (`get_fee_rate`, which takes no arguments beyond
# `env`) to prove the configured contract ID is reachable and callable on the
# configured RPC endpoint. Nothing is written on-chain and no secret key is
# required — `stellar contract invoke --send=no` only simulates the call, and
# `--source-account` accepts a bare public key (G...) for simulation, so no
# funded/signing account is needed.
#
# Requirements:
#   - The `stellar` CLI on PATH (https://developers.stellar.org/docs/tools/cli)
#
# Contract ID resolution (first match wins):
#   1. MARKET_CONTRACT_ID environment variable
#   2. .contracts.market.contractId in deployments/testnet.json (see
#      deployments/README.md for the registry schema)
#
# Optional environment overrides:
#   SOROBAN_RPC_URL      RPC endpoint      (default: from registry, else
#                         https://soroban-testnet.stellar.org)
#   NETWORK_PASSPHRASE   Network passphrase (default: from registry, else
#                         "Test SDF Network ; September 2015")
#   SMOKE_FN             Read-only function to call (default: get_fee_rate)
#   SMOKE_SOURCE_ACCOUNT Public key used to build the simulated transaction
#                         (default: a fixed placeholder G... address — no
#                         secret key or funding required for a view call)
#
# Guard mode:
#   If the `stellar` CLI is missing, or no contract ID is configured via
#   either MARKET_CONTRACT_ID or the registry file, this prints a clear
#   message and exits 0 rather than failing — this script is meant to be
#   safely runnable (e.g. in CI or fresh checkouts) before any real testnet
#   deployment exists.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
REGISTRY_FILE="${ROOT_DIR}/deployments/testnet.json"

SMOKE_FN="${SMOKE_FN:-get_fee_rate}"
# Well-known placeholder public key (does not need to exist/be funded — the
# call is simulated only via --send=no, never signed or submitted).
SMOKE_SOURCE_ACCOUNT="${SMOKE_SOURCE_ACCOUNT:-GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF}"

log() { printf '[testnet-smoke] %s\n' "$*" >&2; }

# ---------------------------------------------------------------------------
# Read a dotted field out of deployments/testnet.json, preferring `jq` and
# falling back to a small inline `node` one-liner. Prints an empty string
# (not an error) if the file, tool, or field is missing.
# ---------------------------------------------------------------------------
read_registry_field() {
  local jq_filter="$1"
  local node_expr="$2"

  if [[ ! -f "${REGISTRY_FILE}" ]]; then
    echo ""
    return 0
  fi

  if command -v jq >/dev/null 2>&1; then
    jq -r "${jq_filter} // empty" "${REGISTRY_FILE}" 2>/dev/null || echo ""
    return 0
  fi

  if command -v node >/dev/null 2>&1; then
    node -e "
      try {
        const fs = require('fs');
        const data = JSON.parse(fs.readFileSync(process.argv[1], 'utf8'));
        const val = (${node_expr});
        if (val !== undefined && val !== null) process.stdout.write(String(val));
      } catch (e) { /* ignore, treat as unset */ }
    " "${REGISTRY_FILE}" 2>/dev/null || echo ""
    return 0
  fi

  # Neither jq nor node available — degrade gracefully to "unset".
  echo ""
}

# 1. Resolve the contract ID: env var wins, else registry file.
CONTRACT_ID="${MARKET_CONTRACT_ID:-}"
if [[ -z "${CONTRACT_ID}" ]]; then
  CONTRACT_ID="$(read_registry_field '.contracts.market.contractId' 'data.contracts && data.contracts.market && data.contracts.market.contractId')"
fi

# 2. Resolve RPC URL and network passphrase: env var wins, else registry,
#    else the well-known Stellar testnet defaults.
RPC_URL="${SOROBAN_RPC_URL:-}"
if [[ -z "${RPC_URL}" ]]; then
  RPC_URL="$(read_registry_field '.rpcUrl' 'data.rpcUrl')"
fi
RPC_URL="${RPC_URL:-https://soroban-testnet.stellar.org}"

PASSPHRASE="${NETWORK_PASSPHRASE:-}"
if [[ -z "${PASSPHRASE}" ]]; then
  PASSPHRASE="$(read_registry_field '.networkPassphrase' 'data.networkPassphrase')"
fi
PASSPHRASE="${PASSPHRASE:-Test SDF Network ; September 2015}"

# 3. Guard mode: no contract ID configured anywhere.
if [[ -z "${CONTRACT_ID}" ]]; then
  log "No market contract ID configured — skipping smoke test (guard mode)."
  log "Set MARKET_CONTRACT_ID, or fill in .contracts.market.contractId in"
  log "${REGISTRY_FILE#"${ROOT_DIR}/"} (see deployments/README.md)."
  exit 0
fi

# 4. Guard mode: stellar CLI missing.
if ! command -v stellar >/dev/null 2>&1; then
  log "'stellar' CLI not found on PATH — skipping smoke test (guard mode)."
  log "Install it from https://developers.stellar.org/docs/tools/cli"
  exit 0
fi

log "Smoke-invoking '${SMOKE_FN}' on contract ${CONTRACT_ID} (read-only, no signing)..."
log "RPC URL: ${RPC_URL}"

# --send=no simulates the call only; it is never signed or submitted, so no
# secret key or funded account is required. --source-account only needs to be
# a syntactically valid public key to build the simulated transaction.
RESULT="$(
  stellar contract invoke \
    --id "${CONTRACT_ID}" \
    --rpc-url "${RPC_URL}" \
    --network-passphrase "${PASSPHRASE}" \
    --source-account "${SMOKE_SOURCE_ACCOUNT}" \
    --send=no \
    -- "${SMOKE_FN}"
)"

log "Smoke invoke succeeded. ${SMOKE_FN} returned: ${RESULT:-<void>}"
echo "${RESULT}"

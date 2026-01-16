#!/usr/bin/env bash
set -euo pipefail

# This script requires bash; don't run it with sh.
if [ -z "${BASH_VERSION:-}" ]; then
  echo "Run with: bash scripts/generate-eden-genesis.sh" >&2
  exit 1
fi

# Generates a genesis file that wires FeeVault + AdminProxy and merges eden.md alloc.
# FeeVault address is deterministic (fixed salt + deployer + fixed config).
# Usage: scripts/generate-eden-genesis.sh [output-path]

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EDEN_MD="${ROOT_DIR}/eden.md"
BASE_GENESIS="${ROOT_DIR}/etc/ev-reth-genesis.json"
OUT_GENESIS="${1:-${ROOT_DIR}/etc/eden-genesis.json}"

ADMIN_PROXY_ADDRESS="0x000000000000000000000000000000000000Ad00"
NTIA_BALANCE_WEI="${NTIA_BALANCE_WEI:-0x0de0b6b3a7640000}"
FEE_VAULT_DEPLOYER="0x4e59b44847b379578588920cA78FbF26c0B4956C"
FEE_VAULT_SALT="0x0000000000000000000000000000000000000000000000000000000000000001"
FEE_VAULT_DESTINATION_DOMAIN="0"
FEE_VAULT_RECIPIENT_ADDRESS="0x0000000000000000000000000000000000000000000000000000000000000000"
FEE_VAULT_MINIMUM_AMOUNT="0"
FEE_VAULT_CALL_FEE="0"
FEE_VAULT_BRIDGE_SHARE_BPS="10000"
FEE_VAULT_OTHER_RECIPIENT="0x0000000000000000000000000000000000000000"
FEE_VAULT_HYP_NATIVE_MINTER="0x0000000000000000000000000000000000000000"

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Missing required command: $1" >&2
    exit 1
  }
}

require_cmd jq
require_cmd cast

if [ ! -f "$EDEN_MD" ]; then
  echo "Missing ${EDEN_MD}" >&2
  exit 1
fi

if [ ! -f "$BASE_GENESIS" ]; then
  echo "Missing ${BASE_GENESIS}" >&2
  exit 1
fi

EOA="$(awk -F 'EOA:' '/^EOA:/ {gsub(/[[:space:]]/, "", $2); print $2; exit}' "$EDEN_MD")"
if [ -z "$EOA" ]; then
  echo "Failed to read EOA from ${EDEN_MD}" >&2
  exit 1
fi

if ! [[ "$EOA" =~ ^0x[0-9a-fA-F]{40}$ ]]; then
  echo "EOA is not a valid address: ${EOA}" >&2
  exit 1
fi

EOA_KEY="$(printf '%s' "${EOA#0x}" | tr '[:upper:]' '[:lower:]')"

EDEN_ALLOC_RAW="$(
  awk '
    /^```json/ {in_block=1; next}
    in_block && /^```/ {exit}
    in_block {print}
  ' "$EDEN_MD"
)"

if [ -z "$EDEN_ALLOC_RAW" ]; then
  echo "Failed to read alloc block from ${EDEN_MD}" >&2
  exit 1
fi

EDEN_ALLOC_CLEAN="$(printf '%s' "$EDEN_ALLOC_RAW" | tr -d '\n' | sed -E 's#//[^"]*##g')"
EDEN_ALLOC_JSON="$(printf '{%s}' "$EDEN_ALLOC_CLEAN")"
EDEN_ALLOC="$(jq -c '.alloc | with_entries(.key |= sub("^0x"; ""))' <<<"$EDEN_ALLOC_JSON")"

ADMIN_PROXY_ARTIFACT="${ROOT_DIR}/contracts/out/AdminProxy.sol/AdminProxy.json"
FEE_VAULT_ARTIFACT="${ROOT_DIR}/contracts/out/FeeVault.sol/FeeVault.json"

if [ ! -f "$ADMIN_PROXY_ARTIFACT" ] || [ ! -f "$FEE_VAULT_ARTIFACT" ]; then
  echo "Missing contract artifacts. Run: (cd ${ROOT_DIR}/contracts && forge build)" >&2
  exit 1
fi

hex_prefix() {
  if [[ "$1" == 0x* ]]; then
    echo "$1"
  else
    echo "0x$1"
  fi
}

pad_hex_32() {
  local hex="${1#0x}"
  printf '0x%064s' "$hex" | tr ' ' '0'
}

pad_hex_20() {
  local hex="${1#0x}"
  printf '%040s' "$hex" | tr ' ' '0'
}

uint_to_hex32() {
  printf '0x%064x' "$1"
}

ADMIN_PROXY_CODE="$(hex_prefix "$(jq -r '.deployedBytecode.object' "$ADMIN_PROXY_ARTIFACT")")"
FEE_VAULT_CODE="$(hex_prefix "$(jq -r '.deployedBytecode.object' "$FEE_VAULT_ARTIFACT")")"
FEE_VAULT_CREATION_CODE="$(hex_prefix "$(jq -r '.bytecode.object' "$FEE_VAULT_ARTIFACT")")"

ADMIN_PROXY_OWNER_SLOT="$(pad_hex_32 "$EOA")"
ADMIN_PROXY_KEY="$(printf '%s' "${ADMIN_PROXY_ADDRESS#0x}" | tr '[:upper:]' '[:lower:]')"
ADMIN_PROXY_ALLOC="$(
  jq -n \
    --arg key "$ADMIN_PROXY_KEY" \
    --arg code "$ADMIN_PROXY_CODE" \
    --arg owner "$ADMIN_PROXY_OWNER_SLOT" \
    --arg slot0 "$SLOT_KEY0" \
    '{($key): {"balance":"0x0","code":$code,"storage":{($slot0):$owner}}}'
)"

DEST_HEX="$(printf '%08x' "$FEE_VAULT_DESTINATION_DOMAIN")"
OWNER_HEX="$(pad_hex_20 "$ADMIN_PROXY_ADDRESS")"
SLOT0="$(pad_hex_32 "$FEE_VAULT_HYP_NATIVE_MINTER")"
SLOT1="0x0000000000000000${DEST_HEX}${OWNER_HEX}"
SLOT2="$(pad_hex_32 "$FEE_VAULT_RECIPIENT_ADDRESS")"
SLOT3="$(uint_to_hex32 "$FEE_VAULT_MINIMUM_AMOUNT")"
SLOT4="$(uint_to_hex32 "$FEE_VAULT_CALL_FEE")"
SLOT5="$(pad_hex_32 "$FEE_VAULT_OTHER_RECIPIENT")"
SLOT6="$(uint_to_hex32 "$FEE_VAULT_BRIDGE_SHARE_BPS")"
SLOT_KEY0="$(uint_to_hex32 0)"
SLOT_KEY1="$(uint_to_hex32 1)"
SLOT_KEY2="$(uint_to_hex32 2)"
SLOT_KEY3="$(uint_to_hex32 3)"
SLOT_KEY4="$(uint_to_hex32 4)"
SLOT_KEY5="$(uint_to_hex32 5)"
SLOT_KEY6="$(uint_to_hex32 6)"

ENCODED_ARGS="$(
  cast abi-encode \
    "f(address,uint32,bytes32,uint256,uint256,uint256,address)" \
    "$ADMIN_PROXY_ADDRESS" \
    "$FEE_VAULT_DESTINATION_DOMAIN" \
    "$FEE_VAULT_RECIPIENT_ADDRESS" \
    "$FEE_VAULT_MINIMUM_AMOUNT" \
    "$FEE_VAULT_CALL_FEE" \
    "$FEE_VAULT_BRIDGE_SHARE_BPS" \
    "$FEE_VAULT_OTHER_RECIPIENT"
)"

INIT_CODE="0x${FEE_VAULT_CREATION_CODE#0x}${ENCODED_ARGS#0x}"
INIT_CODE_HASH="$(cast keccak "$INIT_CODE")"
FEE_VAULT_ADDRESS="$(cast create2 --deployer "$FEE_VAULT_DEPLOYER" --salt "$FEE_VAULT_SALT" --init-code-hash "$INIT_CODE_HASH")"
FEE_VAULT_KEY="$(printf '%s' "${FEE_VAULT_ADDRESS#0x}" | tr '[:upper:]' '[:lower:]')"

FEE_VAULT_ALLOC="$(
  jq -n \
    --arg key "$FEE_VAULT_KEY" \
    --arg code "$FEE_VAULT_CODE" \
    --arg slot0 "$SLOT0" \
    --arg slot1 "$SLOT1" \
    --arg slot2 "$SLOT2" \
    --arg slot3 "$SLOT3" \
    --arg slot4 "$SLOT4" \
    --arg slot5 "$SLOT5" \
    --arg slot6 "$SLOT6" \
    --arg key0 "$SLOT_KEY0" \
    --arg key1 "$SLOT_KEY1" \
    --arg key2 "$SLOT_KEY2" \
    --arg key3 "$SLOT_KEY3" \
    --arg key4 "$SLOT_KEY4" \
    --arg key5 "$SLOT_KEY5" \
    --arg key6 "$SLOT_KEY6" \
    '{($key): {"balance":"0x0","code":$code,"storage":{($key0):$slot0,($key1):$slot1,($key2):$slot2,($key3):$slot3,($key4):$slot4,($key5):$slot5,($key6):$slot6}}}'
)"

EOA_ALLOC="$(jq -n --arg key "$EOA_KEY" --arg balance "$NTIA_BALANCE_WEI" '{($key): {"balance": $balance}}')"

ALLOC="$(
  jq -s '.[0] * .[1] * .[2] * .[3]' \
    <(printf '%s' "$EDEN_ALLOC") \
    <(printf '%s' "$ADMIN_PROXY_ALLOC") \
    <(printf '%s' "$FEE_VAULT_ALLOC") \
    <(printf '%s' "$EOA_ALLOC")
)"

TMP_OUT="${OUT_GENESIS}.tmp"
jq \
  --arg base_fee_sink "$FEE_VAULT_ADDRESS" \
  --arg mint_admin "$ADMIN_PROXY_ADDRESS" \
  --argjson alloc "$ALLOC" \
  '
    .config.evolve.baseFeeSink = $base_fee_sink
    | .config.evolve.baseFeeRedirectActivationHeight = 0
    | .config.evolve.mintAdmin = $mint_admin
    | .alloc = $alloc
  ' \
  "$BASE_GENESIS" > "$TMP_OUT"

mv "$TMP_OUT" "$OUT_GENESIS"

echo "Wrote ${OUT_GENESIS}"
echo "FeeVault address: ${FEE_VAULT_ADDRESS}"
echo "FeeVault deployer: ${FEE_VAULT_DEPLOYER}"
echo "FeeVault salt: ${FEE_VAULT_SALT}"
echo "AdminProxy address: ${ADMIN_PROXY_ADDRESS}"
echo "Admin EOA: ${EOA}"

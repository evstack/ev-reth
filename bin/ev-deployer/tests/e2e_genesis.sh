#!/usr/bin/env bash
# End-to-end test: generate genesis with ev-deployer, boot ev-reth, verify contracts via RPC.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
DEPLOYER="$REPO_ROOT/target/release/ev-deployer"
EV_RETH="$REPO_ROOT/target/release/ev-reth"
CONFIG="$REPO_ROOT/bin/ev-deployer/examples/devnet.toml"
BASE_GENESIS="$REPO_ROOT/bin/ev-dev/assets/devnet-genesis.json"

RPC_PORT=18545
RPC_URL="http://127.0.0.1:$RPC_PORT"
NODE_PID=""
TMPDIR_PATH=""

cleanup() {
    if [[ -n "$NODE_PID" ]]; then
        kill "$NODE_PID" 2>/dev/null || true
        wait "$NODE_PID" 2>/dev/null || true
    fi
    if [[ -n "$TMPDIR_PATH" ]]; then
        rm -rf "$TMPDIR_PATH"
    fi
}
trap cleanup EXIT

# ── Helpers ──────────────────────────────────────────────

fail() { echo "FAIL: $1" >&2; exit 1; }
pass() { echo "PASS: $1"; }

rpc_call() {
    local method="$1"
    local params="$2"
    curl -s --connect-timeout 5 --max-time 10 -X POST "$RPC_URL" \
        -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params,\"id\":1}" \
    | python3 -c "import sys,json; print(json.load(sys.stdin)['result'])"
}

wait_for_rpc() {
    local max_attempts=30
    for i in $(seq 1 $max_attempts); do
        if curl -s --connect-timeout 1 --max-time 2 -X POST "$RPC_URL" \
            -H "Content-Type: application/json" \
            -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
            2>/dev/null | grep -q result; then
            return 0
        fi
        sleep 1
    done
    fail "node did not become ready after ${max_attempts}s"
}

# ── Step 1: Build ────────────────────────────────────────

echo "=== Building ev-deployer and ev-reth ==="
cargo build --release --bin ev-deployer --bin ev-reth --manifest-path "$REPO_ROOT/Cargo.toml" \
    2>&1 | tail -3

[[ -x "$DEPLOYER" ]] || fail "ev-deployer binary not found"
[[ -x "$EV_RETH" ]] || fail "ev-reth binary not found"

# ── Step 2: Generate genesis ─────────────────────────────

TMPDIR_PATH="$(mktemp -d)"
GENESIS="$TMPDIR_PATH/genesis.json"
DATADIR="$TMPDIR_PATH/data"

echo "=== Generating genesis with ev-deployer ==="
"$DEPLOYER" genesis \
    --config "$CONFIG" \
    --merge-into "$BASE_GENESIS" \
    --output "$GENESIS" \
    --force

echo "Genesis written to $GENESIS"

# Quick sanity: address should be in the alloc
grep -q "000000000000000000000000000000000000Ad00" "$GENESIS" \
    || fail "AdminProxy address not found in genesis"
grep -q "000000000000000000000000000000000000FE00" "$GENESIS" \
    || fail "FeeVault address not found in genesis"
grep -q "0000000000000000000000000000000000001100" "$GENESIS" \
    || fail "MerkleTreeHook address not found in genesis"
grep -q "000000000022D473030F116dDEE9F6B43aC78BA3" "$GENESIS" \
    || fail "Permit2 address not found in genesis"

pass "genesis contains all contract addresses"

# ── Step 3: Start ev-reth ────────────────────────────────

echo "=== Starting ev-reth node ==="
"$EV_RETH" node \
    --dev \
    --chain "$GENESIS" \
    --datadir "$DATADIR" \
    --http \
    --http.addr 127.0.0.1 \
    --http.port "$RPC_PORT" \
    --http.api eth,net,web3 \
    --disable-discovery \
    --no-persist-peers \
    --port 0 \
    --log.stdout.filter error \
    &
NODE_PID=$!

echo "Node PID: $NODE_PID, waiting for RPC..."
wait_for_rpc
pass "node is up and responding to RPC"

# ── Step 4: Verify AdminProxy ────────────────────────────

ADMIN_PROXY="0x000000000000000000000000000000000000Ad00"
ADMIN_OWNER="0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"

echo "=== Verifying AdminProxy at $ADMIN_PROXY ==="

# Check code is present
admin_code=$(rpc_call "eth_getCode" "[\"$ADMIN_PROXY\", \"latest\"]")
[[ "$admin_code" != "0x" && "$admin_code" != "0x0" && ${#admin_code} -gt 10 ]] \
    || fail "AdminProxy has no bytecode (got: $admin_code)"
pass "AdminProxy has bytecode (${#admin_code} hex chars)"

# Check owner in slot 0
admin_slot0=$(rpc_call "eth_getStorageAt" "[\"$ADMIN_PROXY\", \"0x0\", \"latest\"]")
# Owner should be in the lower 20 bytes, left-padded to 32 bytes
expected_owner_slot="0x000000000000000000000000f39fd6e51aad88f6f4ce6ab8827279cfffb92266"
[[ "$(echo "$admin_slot0" | tr '[:upper:]' '[:lower:]')" == "$(echo "$expected_owner_slot" | tr '[:upper:]' '[:lower:]')" ]] \
    || fail "AdminProxy slot 0 (owner) mismatch: got $admin_slot0, expected $expected_owner_slot"
pass "AdminProxy owner slot 0 = $ADMIN_OWNER"

# ── Step 5: Verify FeeVault ──────────────────────────────

FEE_VAULT="0x000000000000000000000000000000000000FE00"
FEE_VAULT_OWNER="0x000000000000000000000000000000000000Ad00"

echo "=== Verifying FeeVault at $FEE_VAULT ==="

# Check code is present
fv_code=$(rpc_call "eth_getCode" "[\"$FEE_VAULT\", \"latest\"]")
[[ "$fv_code" != "0x" && "$fv_code" != "0x0" && ${#fv_code} -gt 10 ]] \
    || fail "FeeVault has no bytecode (got: $fv_code)"
pass "FeeVault has bytecode (${#fv_code} hex chars)"

# Slot 0: hypNativeMinter (should be zero)
fv_slot0=$(rpc_call "eth_getStorageAt" "[\"$FEE_VAULT\", \"0x0\", \"latest\"]")
expected_zero="0x0000000000000000000000000000000000000000000000000000000000000000"
[[ "$(echo "$fv_slot0" | tr '[:upper:]' '[:lower:]')" == "$(echo "$expected_zero" | tr '[:upper:]' '[:lower:]')" ]] \
    || fail "FeeVault slot 0 (hypNativeMinter) should be zero, got $fv_slot0"
pass "FeeVault slot 0 (hypNativeMinter) = zero"

# Slot 1: owner (lower 160 bits) + destinationDomain (upper bits)
# With domain=0 and owner=0x...Ad00, it's just the owner padded
fv_slot1=$(rpc_call "eth_getStorageAt" "[\"$FEE_VAULT\", \"0x1\", \"latest\"]")
expected_slot1="0x000000000000000000000000000000000000000000000000000000000000ad00"
[[ "$(echo "$fv_slot1" | tr '[:upper:]' '[:lower:]')" == "$(echo "$expected_slot1" | tr '[:upper:]' '[:lower:]')" ]] \
    || fail "FeeVault slot 1 (owner|domain) mismatch: got $fv_slot1, expected $expected_slot1"
pass "FeeVault slot 1 (owner|domain) correct"

# Slot 6: bridgeShareBps = 10000 = 0x2710
fv_slot6=$(rpc_call "eth_getStorageAt" "[\"$FEE_VAULT\", \"0x6\", \"latest\"]")
expected_slot6="0x0000000000000000000000000000000000000000000000000000000000002710"
[[ "$(echo "$fv_slot6" | tr '[:upper:]' '[:lower:]')" == "$(echo "$expected_slot6" | tr '[:upper:]' '[:lower:]')" ]] \
    || fail "FeeVault slot 6 (bridgeShareBps) mismatch: got $fv_slot6, expected $expected_slot6"
pass "FeeVault slot 6 (bridgeShareBps) = 10000"

# ── Step 6: Verify MerkleTreeHook ────────────────────────

MERKLE_TREE_HOOK="0x0000000000000000000000000000000000001100"
MERKLE_TREE_HOOK_OWNER="0x000000000000000000000000000000000000Ad00"
MERKLE_TREE_HOOK_MAILBOX="0x0000000000000000000000000000000000001200"

echo "=== Verifying MerkleTreeHook at $MERKLE_TREE_HOOK ==="

# Check code is present
mth_code=$(rpc_call "eth_getCode" "[\"$MERKLE_TREE_HOOK\", \"latest\"]")
[[ "$mth_code" != "0x" && "$mth_code" != "0x0" && ${#mth_code} -gt 10 ]] \
    || fail "MerkleTreeHook has no bytecode (got: $mth_code)"
pass "MerkleTreeHook has bytecode (${#mth_code} hex chars)"

# Compare full bytecode against genesis JSON
# Extract expected code from genesis for the MerkleTreeHook address
expected_mth_code=$(python3 -c "
import json, sys
with open('$GENESIS') as f:
    genesis = json.load(f)
alloc = genesis['alloc']
# Address key is checksummed without 0x prefix
entry = alloc.get('0000000000000000000000000000000000001100')
print(entry['code'])
")
[[ "$(echo "$mth_code" | tr '[:upper:]' '[:lower:]')" == "$(echo "$expected_mth_code" | tr '[:upper:]' '[:lower:]')" ]] \
    || fail "MerkleTreeHook bytecode from node does not match genesis JSON"
pass "MerkleTreeHook bytecode matches genesis JSON"

# Slot 0: _initialized = 1 (OZ v4 Initializable)
mth_slot0=$(rpc_call "eth_getStorageAt" "[\"$MERKLE_TREE_HOOK\", \"0x0\", \"latest\"]")
expected_init="0x0000000000000000000000000000000000000000000000000000000000000001"
[[ "$(echo "$mth_slot0" | tr '[:upper:]' '[:lower:]')" == "$(echo "$expected_init" | tr '[:upper:]' '[:lower:]')" ]] \
    || fail "MerkleTreeHook slot 0 (_initialized) mismatch: got $mth_slot0, expected $expected_init"
pass "MerkleTreeHook slot 0 (_initialized) = 1"

# Slot 51 (0x33): _owner
mth_slot51=$(rpc_call "eth_getStorageAt" "[\"$MERKLE_TREE_HOOK\", \"0x33\", \"latest\"]")
expected_owner="0x000000000000000000000000000000000000000000000000000000000000ad00"
[[ "$(echo "$mth_slot51" | tr '[:upper:]' '[:lower:]')" == "$(echo "$expected_owner" | tr '[:upper:]' '[:lower:]')" ]] \
    || fail "MerkleTreeHook slot 51 (_owner) mismatch: got $mth_slot51, expected $expected_owner"
pass "MerkleTreeHook slot 51 (_owner) = $MERKLE_TREE_HOOK_OWNER"

# Verify immutables are patched in bytecode:
# mailbox address at byte offsets 904 and 3300 (each is a 32-byte word, address in last 20 bytes)
# localDomain (chain_id=1234=0x04d2) at byte offset 644
# The hex string has "0x" prefix, so byte N in the bytecode = hex chars at positions 2+2N..2+2N+2
mth_hex="${mth_code#0x}"

check_immutable() {
    local name="$1"
    local byte_offset="$2"
    local expected_hex="$3"
    local hex_offset=$((byte_offset * 2))
    local hex_len=${#expected_hex}
    local actual="${mth_hex:$hex_offset:$hex_len}"
    [[ "$(echo "$actual" | tr '[:upper:]' '[:lower:]')" == "$(echo "$expected_hex" | tr '[:upper:]' '[:lower:]')" ]] \
        || fail "$name at byte offset $byte_offset mismatch: got $actual, expected $expected_hex"
    pass "$name patched correctly at byte offset $byte_offset"
}

# mailbox = 0x...1200 → 32-byte word with address in last 20 bytes
# Full 32-byte word: 000000000000000000000000 + 0000000000000000000000000000000000001200
mailbox_word="0000000000000000000000000000000000000000000000000000000000001200"
check_immutable "mailbox" 904 "$mailbox_word"
check_immutable "mailbox (second ref)" 3300 "$mailbox_word"

# localDomain = chain_id 1234 = 0x04d2 → 32-byte word
domain_word="00000000000000000000000000000000000000000000000000000000000004d2"
check_immutable "localDomain" 644 "$domain_word"

# deployedBlock = 0 → 32 zero bytes
deployed_block_word="0000000000000000000000000000000000000000000000000000000000000000"
check_immutable "deployedBlock" 578 "$deployed_block_word"

# ── Step 7: Verify Permit2 ─────────────────────────────

PERMIT2="0x000000000022D473030F116dDEE9F6B43aC78BA3"

echo "=== Verifying Permit2 at $PERMIT2 ==="

# Check code is present
p2_code=$(rpc_call "eth_getCode" "[\"$PERMIT2\", \"latest\"]")
[[ "$p2_code" != "0x" && "$p2_code" != "0x0" && ${#p2_code} -gt 10 ]] \
    || fail "Permit2 has no bytecode (got: $p2_code)"
pass "Permit2 has bytecode (${#p2_code} hex chars)"

# Call DOMAIN_SEPARATOR() — selector 0x3644e515
# Should return the cached domain separator matching chain_id=1234 and the contract address
p2_domain_sep=$(rpc_call "eth_call" "[{\"to\":\"$PERMIT2\",\"data\":\"0x3644e515\"}, \"latest\"]")
expected_domain_sep="0x6cda538cafce36292a6ef27740629597f85f6716f5694d26d5c59fc1d07cfd95"
[[ "$(echo "$p2_domain_sep" | tr '[:upper:]' '[:lower:]')" == "$(echo "$expected_domain_sep" | tr '[:upper:]' '[:lower:]')" ]] \
    || fail "Permit2 DOMAIN_SEPARATOR() mismatch: got $p2_domain_sep, expected $expected_domain_sep"
pass "Permit2 DOMAIN_SEPARATOR() correct for chain_id=1234"

# ── Done ─────────────────────────────────────────────────

echo ""
echo "=== All checks passed ==="

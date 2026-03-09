#!/usr/bin/env bash
set -euo pipefail

# ===============================================================
# Test Eden Hardfork Upgrade Path
# ===============================================================
# Tests that upgrading from the main binary to the eden binary
# properly applies WTIA storage changes at the hardfork height.
#
# Flow:
# 1. Build binary from current branch (eden hardfork support)
# 2. Build binary from main (no eden hardfork)
# 3. Start main binary, produce 3 blocks
# 4. Verify WTIA storage is empty
# 5. Stop node
# 6. Start eden binary with --eden-hardfork-height 5
# 7. Produce blocks 4 and 5
# 8. Verify WTIA storage has NAME, SYMBOL, DECIMALS
# ===============================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
CURRENT_BRANCH=$(git -C "$PROJECT_DIR" rev-parse --abbrev-ref HEAD)

# Test configuration
HARDFORK_HEIGHT=5
BLOCKS_BEFORE_UPGRADE=3
BLOCKS_AFTER_UPGRADE=2

# Network ports (non-standard to avoid conflicts)
HTTP_PORT=28545
AUTH_PORT=28551
P2P_PORT=30304

# WTIA Contract
WTIA_ADDRESS="0x00000000000000000000000000000000Ce1e571A"

# Expected storage values (Solidity short string encoding)
EXPECTED_NAME="0x5772617070656420544941000000000000000000000000000000000000000016"
EXPECTED_SYMBOL="0x5754494100000000000000000000000000000000000000000000000000000008"
EXPECTED_DECIMALS="0x0000000000000000000000000000000000000000000000000000000000000012"

# Temp directories
WORK_DIR=$(mktemp -d)
DATADIR="$WORK_DIR/data"
JWT_SECRET_FILE="$WORK_DIR/jwt.hex"
GENESIS="$SCRIPT_DIR/eden-genesis.json"

# Binary paths
MAIN_BINARY="$WORK_DIR/ev-reth-main"
EDEN_BINARY="$WORK_DIR/ev-reth-eden"

NODE_PID=""
MAIN_WORKTREE=""

# Block tracking (global state)
HEAD_HASH=""
HEAD_NUMBER=0
HEAD_TIMESTAMP=0

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

cleanup() {
    echo ""
    echo -e "${YELLOW}Cleaning up...${NC}"
    if [ -n "$NODE_PID" ] && kill -0 "$NODE_PID" 2>/dev/null; then
        kill "$NODE_PID" 2>/dev/null || true
        wait "$NODE_PID" 2>/dev/null || true
    fi
    # Remove temp worktree if it was created
    if [ -n "$MAIN_WORKTREE" ] && [ -d "$MAIN_WORKTREE" ]; then
        git -C "$PROJECT_DIR" worktree remove "$MAIN_WORKTREE" --force 2>/dev/null || true
    fi
    echo "Work dir: $WORK_DIR (not deleted - check node.log if needed)"
    echo "To clean up: rm -rf $WORK_DIR"
}
trap cleanup EXIT

log() { echo -e "${GREEN}>>>${NC} $1"; }
info() { echo -e "${BLUE}   $1${NC}"; }
fail() { echo -e "${RED}[FAIL]${NC} $1"; }

die() {
    echo -e "${RED}[ERROR]${NC} $1"
    if [ -f "$WORK_DIR/node.log" ]; then
        echo -e "${YELLOW}Last 20 lines of node log:${NC}"
        tail -20 "$WORK_DIR/node.log"
    fi
    exit 1
}

# --------------- JWT & RPC helpers ---------------

generate_jwt() {
    python3 -c "
import hmac, hashlib, base64, json, time
secret = bytes.fromhex(open('$JWT_SECRET_FILE').read().strip())
header = base64.urlsafe_b64encode(json.dumps({'alg':'HS256','typ':'JWT'}).encode()).rstrip(b'=').decode()
payload = base64.urlsafe_b64encode(json.dumps({'iat':int(time.time())}).encode()).rstrip(b'=').decode()
unsigned = f'{header}.{payload}'
sig = base64.urlsafe_b64encode(hmac.new(secret, unsigned.encode(), hashlib.sha256).digest()).rstrip(b'=').decode()
print(f'{unsigned}.{sig}')
"
}

engine_call() {
    local method="$1"
    local params="$2"
    local jwt
    jwt=$(generate_jwt)
    curl -s -X POST \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer $jwt" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params,\"id\":1}" \
        "http://localhost:$AUTH_PORT"
}

rpc_call() {
    local method="$1"
    local params="$2"
    curl -s -X POST \
        -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params,\"id\":1}" \
        "http://localhost:$HTTP_PORT"
}

# --------------- Node lifecycle ---------------

wait_for_node() {
    local max=60
    local i=0
    while [ $i -lt $max ]; do
        if rpc_call "eth_blockNumber" "[]" 2>/dev/null | jq -e '.result' >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
        i=$((i + 1))
    done
    die "Node failed to become ready after ${max}s"
}

start_node() {
    local binary="$1"
    shift

    mkdir -p "$DATADIR"

    RUST_LOG="info,ev_reth=debug,ev_node=debug" \
    "$binary" node \
        --chain "$GENESIS" \
        --datadir "$DATADIR" \
        --http \
        --http.port "$HTTP_PORT" \
        --authrpc.port "$AUTH_PORT" \
        --authrpc.jwtsecret "$JWT_SECRET_FILE" \
        --port "$P2P_PORT" \
        "$@" \
        >> "$WORK_DIR/node.log" 2>&1 &
    NODE_PID=$!

    log "Node started (PID: $NODE_PID, binary: $(basename "$binary"))"
    # Quick check: if the process died immediately, show the log
    sleep 2
    if ! kill -0 "$NODE_PID" 2>/dev/null; then
        fail "Node process died immediately!"
        echo -e "${YELLOW}Last 30 lines of node log:${NC}"
        tail -30 "$WORK_DIR/node.log"
        die "Node crashed on startup"
    fi
    wait_for_node
    log "Node is ready"
}

stop_node() {
    if [ -n "$NODE_PID" ] && kill -0 "$NODE_PID" 2>/dev/null; then
        log "Stopping node (PID: $NODE_PID)..."
        kill "$NODE_PID"
        wait "$NODE_PID" 2>/dev/null || true
        NODE_PID=""
        sleep 2
        log "Node stopped"
    fi
}

# --------------- Block production via Engine API ---------------

load_head() {
    local block
    block=$(rpc_call "eth_getBlockByNumber" "[\"latest\", false]" | jq -r '.result')
    HEAD_HASH=$(echo "$block" | jq -r '.hash')
    HEAD_NUMBER=$(printf "%d" "$(echo "$block" | jq -r '.number')")
    HEAD_TIMESTAMP=$(printf "%d" "$(echo "$block" | jq -r '.timestamp')")
}

build_block() {
    local next_ts=$((HEAD_TIMESTAMP + 12))
    local next_ts_hex
    next_ts_hex=$(printf "0x%x" $next_ts)
    local prev_randao="0x$(openssl rand -hex 32)"
    local fee_recipient="0x0000000000000000000000000000000000000000"
    local beacon_root="0x0000000000000000000000000000000000000000000000000000000000000000"

    info "Building block $((HEAD_NUMBER + 1))..."

    # 1) forkchoiceUpdatedV3 — request new payload
    local fcu_resp
    fcu_resp=$(engine_call "engine_forkchoiceUpdatedV3" "[
        {\"headBlockHash\":\"$HEAD_HASH\",\"safeBlockHash\":\"$HEAD_HASH\",\"finalizedBlockHash\":\"$HEAD_HASH\"},
        {\"timestamp\":\"$next_ts_hex\",\"prevRandao\":\"$prev_randao\",\"suggestedFeeRecipient\":\"$fee_recipient\",\"withdrawals\":[],\"parentBeaconBlockRoot\":\"$beacon_root\"}
    ]")

    local status
    status=$(echo "$fcu_resp" | jq -r '.result.payloadStatus.status')
    local payload_id
    payload_id=$(echo "$fcu_resp" | jq -r '.result.payloadId')

    if [ "$status" != "VALID" ]; then
        echo "$fcu_resp" | jq .
        die "forkchoiceUpdatedV3 status=$status (expected VALID)"
    fi
    [ "$payload_id" != "null" ] && [ -n "$payload_id" ] || die "No payloadId returned"

    # 2) getPayload — try V5 (Osaka), V4 (Prague), V3 (Cancun) in order
    sleep 1
    local gp_resp gp_error gp_version
    for gp_version in 5 4 3; do
        gp_resp=$(engine_call "engine_getPayloadV${gp_version}" "[\"$payload_id\"]")
        gp_error=$(echo "$gp_resp" | jq -r '.error.message // empty')
        if [ -z "$gp_error" ]; then
            break
        fi
        info "getPayloadV${gp_version} failed ($gp_error), trying next..."
    done
    [ -z "$gp_error" ] || die "All getPayload versions failed: $gp_error"

    local block_hash
    block_hash=$(echo "$gp_resp" | jq -r '.result.executionPayload.blockHash')
    [ "$block_hash" != "null" ] && [ -n "$block_hash" ] || {
        echo "$gp_resp" | jq .
        die "No blockHash in getPayloadV${gp_version} response"
    }

    local exec_payload
    exec_payload=$(echo "$gp_resp" | jq '.result.executionPayload')

    # Extract executionRequests if present (Prague/Osaka V4+)
    local exec_requests
    exec_requests=$(echo "$gp_resp" | jq '.result.executionRequests // empty')

    # 3) newPayload — match the version from getPayload
    local np_resp np_error np_version
    if [ -n "$exec_requests" ] && [ "$exec_requests" != "null" ]; then
        # Try V5, V4 with executionRequests
        for np_version in 5 4; do
            np_resp=$(engine_call "engine_newPayloadV${np_version}" "[$exec_payload, [], \"$beacon_root\", $exec_requests]")
            np_error=$(echo "$np_resp" | jq -r '.error.message // empty')
            if [ -z "$np_error" ]; then
                break
            fi
            info "newPayloadV${np_version} failed ($np_error), trying next..."
        done
    else
        np_resp=$(engine_call "engine_newPayloadV3" "[$exec_payload, [], \"$beacon_root\"]")
        np_error=$(echo "$np_resp" | jq -r '.error.message // empty')
    fi

    local np_status
    np_status=$(echo "$np_resp" | jq -r '.result.status')
    if [ "$np_status" != "VALID" ]; then
        echo "$np_resp" | jq .
        die "newPayload status=$np_status (expected VALID)"
    fi

    # 4) forkchoiceUpdatedV3 — finalize
    local fin_resp
    fin_resp=$(engine_call "engine_forkchoiceUpdatedV3" "[
        {\"headBlockHash\":\"$block_hash\",\"safeBlockHash\":\"$block_hash\",\"finalizedBlockHash\":\"$block_hash\"},
        null
    ]")

    local fin_status
    fin_status=$(echo "$fin_resp" | jq -r '.result.payloadStatus.status')
    if [ "$fin_status" != "VALID" ]; then
        echo "$fin_resp" | jq .
        die "Finalize forkchoiceUpdatedV3 status=$fin_status"
    fi

    # Update head
    HEAD_HASH="$block_hash"
    HEAD_NUMBER=$((HEAD_NUMBER + 1))
    HEAD_TIMESTAMP=$next_ts

    info "Block $HEAD_NUMBER ok (${HEAD_HASH:0:18}...)"
}

# --------------- Storage checks ---------------

get_storage() {
    local slot="$1"
    rpc_call "eth_getStorageAt" "[\"$WTIA_ADDRESS\", \"$slot\", \"latest\"]" | jq -r '.result'
}

# Normalize hex to 66-char (0x + 64 hex digits)
normalize_hex() {
    local val="$1"
    # Remove 0x prefix
    val="${val#0x}"
    # Pad to 64 chars
    printf "0x%064s" "$val" | tr ' ' '0'
}

# ===============================================================
# MAIN
# ===============================================================

echo ""
echo "============================================"
echo "  Eden Hardfork Upgrade Path Test"
echo "============================================"
echo ""
log "Branch:          $CURRENT_BRANCH"
log "Hardfork height: $HARDFORK_HEIGHT"
log "Work dir:        $WORK_DIR"
echo ""

# --- Step 1: Build binaries ---
log "STEP 1: Building binaries"

log "Building eden binary ($CURRENT_BRANCH)..."
cd "$PROJECT_DIR"
cargo build --release --bin ev-reth 2>&1 | tail -5
cp "$PROJECT_DIR/target/release/ev-reth" "$EDEN_BINARY"
info "Saved: $EDEN_BINARY"

log "Building main binary (via temporary worktree)..."
MAIN_WORKTREE="$WORK_DIR/main-worktree"
# Use --detach to avoid conflicts when 'main' is checked out in another worktree
git -C "$PROJECT_DIR" worktree add --detach "$MAIN_WORKTREE" main --quiet
# Share CARGO_TARGET_DIR so we reuse the compilation cache
CARGO_TARGET_DIR="$PROJECT_DIR/target" cargo build --release --bin ev-reth \
    --manifest-path "$MAIN_WORKTREE/Cargo.toml" 2>&1 | tail -5
cp "$PROJECT_DIR/target/release/ev-reth" "$MAIN_BINARY"
info "Saved: $MAIN_BINARY"
# Clean up worktree immediately
git -C "$PROJECT_DIR" worktree remove "$MAIN_WORKTREE" --force 2>/dev/null || true
MAIN_WORKTREE=""
echo ""

# --- Step 2: Setup ---
log "STEP 2: Setup"
openssl rand -hex 32 > "$JWT_SECRET_FILE"
info "JWT secret: $JWT_SECRET_FILE"
info "Genesis:    $GENESIS"
info "Datadir:    $DATADIR"
echo ""

# --- Step 3: Run main binary, produce blocks ---
log "STEP 3: Start main binary, produce $BLOCKS_BEFORE_UPGRADE blocks"
start_node "$MAIN_BINARY"
load_head
info "Genesis: block=$HEAD_NUMBER hash=${HEAD_HASH:0:18}..."

for _ in $(seq 1 "$BLOCKS_BEFORE_UPGRADE"); do
    build_block
done
echo ""

# --- Step 4: Verify storage empty ---
log "STEP 4: Verify WTIA storage is empty (before hardfork)"
NAME_VAL=$(normalize_hex "$(get_storage '0x0')")
SYMBOL_VAL=$(normalize_hex "$(get_storage '0x1')")
DECIMALS_VAL=$(normalize_hex "$(get_storage '0x2')")
ZERO="0x0000000000000000000000000000000000000000000000000000000000000000"

if [ "$NAME_VAL" = "$ZERO" ] && [ "$SYMBOL_VAL" = "$ZERO" ] && [ "$DECIMALS_VAL" = "$ZERO" ]; then
    log "PASS: WTIA storage is empty before hardfork"
else
    fail "WTIA storage is NOT empty before hardfork!"
    info "name=$NAME_VAL symbol=$SYMBOL_VAL decimals=$DECIMALS_VAL"
    die "Pre-hardfork check failed"
fi
echo ""

# --- Step 5: Stop node ---
log "STEP 5: Stop main binary"
stop_node
echo ""

# --- Step 6: Start eden binary ---
log "STEP 6: Start eden binary with --eden-hardfork-height $HARDFORK_HEIGHT"
start_node "$EDEN_BINARY" "--eden-hardfork-height" "$HARDFORK_HEIGHT"

# After restart, the consensus engine doesn't remember the head.
# Blocks are in the DB, but we must tell the engine via forkchoiceUpdatedV3.
info "Restoring chain head to block $HEAD_NUMBER (${HEAD_HASH:0:18}...)..."
local_beacon_root="0x0000000000000000000000000000000000000000000000000000000000000000"
restore_resp=$(engine_call "engine_forkchoiceUpdatedV3" "[
    {\"headBlockHash\":\"$HEAD_HASH\",\"safeBlockHash\":\"$HEAD_HASH\",\"finalizedBlockHash\":\"$HEAD_HASH\"},
    null
]")
restore_status=$(echo "$restore_resp" | jq -r '.result.payloadStatus.status')
if [ "$restore_status" != "VALID" ]; then
    echo "$restore_resp" | jq .
    die "Failed to restore chain head after restart (status=$restore_status)"
fi

# Verify head is correct now
load_head
info "Resumed at block $HEAD_NUMBER"

if [ "$HEAD_NUMBER" -ne "$BLOCKS_BEFORE_UPGRADE" ]; then
    die "Expected block $BLOCKS_BEFORE_UPGRADE, got $HEAD_NUMBER"
fi
echo ""

# --- Step 7: Produce blocks to reach hardfork ---
log "STEP 7: Produce $BLOCKS_AFTER_UPGRADE blocks (reaching hardfork at block $HARDFORK_HEIGHT)"
for _ in $(seq 1 "$BLOCKS_AFTER_UPGRADE"); do
    build_block
done
info "Current head: block $HEAD_NUMBER"
echo ""

# --- Step 8: Verify storage after hardfork ---
log "STEP 8: Verify WTIA storage AFTER hardfork"
NAME_VAL=$(normalize_hex "$(get_storage '0x0')")
SYMBOL_VAL=$(normalize_hex "$(get_storage '0x1')")
DECIMALS_VAL=$(normalize_hex "$(get_storage '0x2')")

info "NAME     (slot 0): $NAME_VAL"
info "SYMBOL   (slot 1): $SYMBOL_VAL"
info "DECIMALS (slot 2): $DECIMALS_VAL"
echo ""

ALL_PASS=true

if [ "$NAME_VAL" = "$EXPECTED_NAME" ]; then
    log "PASS: NAME = 'Wrapped TIA'"
else
    fail "NAME mismatch"
    info "expected: $EXPECTED_NAME"
    info "got:      $NAME_VAL"
    ALL_PASS=false
fi

if [ "$SYMBOL_VAL" = "$EXPECTED_SYMBOL" ]; then
    log "PASS: SYMBOL = 'WTIA'"
else
    fail "SYMBOL mismatch"
    info "expected: $EXPECTED_SYMBOL"
    info "got:      $SYMBOL_VAL"
    ALL_PASS=false
fi

if [ "$DECIMALS_VAL" = "$EXPECTED_DECIMALS" ]; then
    log "PASS: DECIMALS = 18"
else
    fail "DECIMALS mismatch"
    info "expected: $EXPECTED_DECIMALS"
    info "got:      $DECIMALS_VAL"
    ALL_PASS=false
fi

# --- Cleanup ---
stop_node

echo ""
echo "============================================"
if [ "$ALL_PASS" = true ]; then
    echo -e "  ${GREEN}ALL TESTS PASSED${NC}"
    echo "============================================"
    exit 0
else
    echo -e "  ${RED}SOME TESTS FAILED${NC}"
    echo "============================================"
    echo "Check node log: $WORK_DIR/node.log"
    exit 1
fi

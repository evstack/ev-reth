#!/usr/bin/env bash
# End-to-end test: generate genesis with ev-deployer, boot ev-reth, verify merge works via RPC.
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

# Sanity: output should be valid JSON with alloc field
python3 -c "import sys,json; g=json.load(open('$GENESIS')); assert 'alloc' in g" \
    || fail "genesis output is not valid JSON or missing alloc"

pass "genesis merge produced valid JSON with alloc"

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

# ── Step 4: Verify node boots with merged genesis ────────

block_number=$(rpc_call "eth_blockNumber" "[]")
[[ -n "$block_number" ]] || fail "could not get block number from node"
pass "node booted successfully with merged genesis (block: $block_number)"

# ── Done ─────────────────────────────────────────────────

echo ""
echo "=== All checks passed ==="

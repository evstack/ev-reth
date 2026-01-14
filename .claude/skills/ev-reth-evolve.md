---
description: This skill should be used when the user asks about "Evolve integration", "payload attributes", "EvolvePayloadAttributes", "consensus modifications", "txpoolExt RPC", "how transactions flow through Evolve", or wants to understand how ev-reth connects to the Evolve system.
---

# Evolve Integration Onboarding

## Overview

Evolve-specific integration lives in `crates/evolve/`. This handles the protocol-level differences between standard Ethereum and Evolve.

## Key Files

### Types
- `crates/evolve/src/types.rs` - `EvolvePayloadAttributes`, validation errors
- `crates/evolve/src/lib.rs` - Public API exports

### Configuration
- `crates/evolve/src/config.rs` - `EvolveConfig`, txpool limits

### Consensus
- `crates/evolve/src/consensus.rs` - `EvolveConsensus` modifications

### RPC Extensions
- `crates/evolve/src/rpc/mod.rs` - RPC module exports
- `crates/evolve/src/rpc/txpool.rs` - `EvolveTxpoolApiServer` extension

## Architecture

### Payload Attributes

`EvolvePayloadAttributes` (from `crates/evolve/src/types.rs:7-22`):

```rust
pub struct EvolvePayloadAttributes {
    pub transactions: Vec<TransactionSigned>,  // Signed transactions (not raw bytes)
    pub gas_limit: Option<u64>,                // Optional gas limit
    pub timestamp: u64,                        // Block timestamp
    pub prev_randao: B256,                     // Prev randao value
    pub suggested_fee_recipient: Address,      // Fee recipient
    pub parent_hash: B256,                     // Parent block hash
    pub block_number: u64,                     // Block number
}
```

This is the key innovation: transactions are submitted through `engine_forkchoiceUpdatedV3` rather than pulled from a mempool.

### Consensus Modifications

`EvolveConsensus` wraps `EthBeaconConsensus` with relaxed rules. The key difference:
- Allows blocks with equal timestamps (`>=` instead of `>`)
- Standard Ethereum requires strictly increasing timestamps

This is needed because Evolve may produce multiple blocks with the same timestamp.

### RPC Extensions

`EvolveTxpoolApiServer` adds custom txpool RPC methods. See `crates/evolve/src/rpc/txpool.rs:10-15`:

```rust
#[rpc(server, namespace = "txpoolExt")]
pub trait EvolveTxpoolApi {
    /// Get transactions from the pool up to the configured limits
    #[method(name = "getTxs")]
    async fn get_txs(&self) -> RpcResult<Vec<Bytes>>;
}
```

Note: `get_txs` takes no parameters - limits are configured at API creation time via `EvolveConfig`.

## Transaction Flow

```
Evolve submits transactions
    ↓
engine_forkchoiceUpdatedV3 with EvolvePayloadAttributes
    ↓
Transactions (Vec<TransactionSigned>) validated
    ↓
Passed to EvolvePayloadBuilder
    ↓
Executed and included in block
```

## Key Design Decisions

1. **Direct Submission** - No mempool, transactions in payload attributes
2. **Equal Timestamps** - Consensus allows same-timestamp blocks
3. **RPC Extensions** - Custom namespace for Evolve-specific operations
4. **Signed Transactions** - Payload attributes contain `TransactionSigned`, not raw bytes

## Connection to Other Components

- **crates/node** - Uses `EvolvePayloadAttributes` in builder
- **crates/ev-revm** - Executes transactions from attributes
- **bin/ev-reth** - Registers RPC extensions

## Development Commands

```bash
make test-evolve
# Or directly:
cargo test -p evolve-ev-reth
```

## Exploration Starting Points

1. Start with `crates/evolve/src/types.rs` for payload attributes
2. Read `crates/evolve/src/consensus.rs` for consensus modifications
3. Check `crates/evolve/src/rpc/txpool.rs` for RPC extensions
4. See `crates/node/src/builder.rs` for how attributes are processed

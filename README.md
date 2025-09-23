# EV-reth - Evolve Integration for Reth

EV-reth is a specialized integration layer that enables [Reth](https://github.com/paradigmxyz/reth) to work seamlessly with Evolve, providing a custom payload builder that supports transaction submission via the Engine API.

## Overview

This project provides a modified version of Reth that includes:

- **Custom Payload Builder**: A specialized payload builder that accepts transactions through Engine API payload attributes
- **Evolve-Compatible Engine API**: Modified Engine API validation to work with Evolve's block production model
- **Transaction Support**: Full support for including transactions in blocks via the Engine API `engine_forkchoiceUpdatedV3` method
- **Custom Consensus**: Modified consensus layer that allows multiple blocks to have the same timestamp
- **Txpool RPC Extension**: Custom `txpoolExt_getTxs` RPC method for efficient transaction retrieval with configurable size limits

## Key Features

### 1. Engine API Transaction Support

Unlike standard Reth, ev-reth accepts transactions directly through the Engine API payload attributes. This allows Evolve to submit transactions when requesting new payload creation.

### 2. Custom Payload Builder

The `EvolvePayloadBuilder` handles:

- Transaction decoding from Engine API attributes
- Block construction with proper gas limits
- State execution and validation

### 3. Flexible Block Validation

Modified Engine API validator that:

- Bypasses block hash validation for Evolve blocks
- Supports custom gas limits per payload
- Maintains compatibility with standard Ethereum validation where possible

### 4. Custom Consensus for Equal Timestamps

ev-reth includes a custom consensus implementation (`EvolveConsensus`) that:

- Allows multiple blocks to have the same timestamp
- Wraps the standard Ethereum beacon consensus for most validation
- Only modifies timestamp validation to accept `header.timestamp >= parent.timestamp` instead of requiring strictly greater timestamps
- Essential for Evolve's operation where multiple blocks may be produced with the same timestamp

### 5. Txpool RPC Extension

Custom RPC namespace `txpoolExt` that provides:

- `txpoolExt_getTxs`: Retrieves pending transactions from the pool as RLP-encoded bytes
- Configurable byte limit for transaction retrieval (default: 1.98 MB)
- Efficient iteration that stops when reaching the byte limit

## Installation

### Prerequisites

- Rust 1.82 or higher
- Git

### Building from Source

```bash
# Clone the repository
git clone https://github.com/evstack/ev-reth.git
cd ev-reth

# Build the project
make build

# Run tests
make test
```

## Usage

### Running the ev-reth Node

Basic usage:

```bash
./target/release/ev-reth node
```

With custom configuration:

```bash
./target/release/ev-reth node \
    --chain <CHAIN_SPEC> \
    --datadir <DATA_DIR> \
    --http \
    --http.api all \
    --ws \
    --ws.api all
```

### Engine API Integration

When using the Engine API, you can include transactions in the payload attributes:

```json
{
  "method": "engine_forkchoiceUpdatedV3",
  "params": [
    {
      "headBlockHash": "0x...",
      "safeBlockHash": "0x...",
      "finalizedBlockHash": "0x..."
    },
    {
      "timestamp": "0x...",
      "prevRandao": "0x...",
      "suggestedFeeRecipient": "0x...",
      "withdrawals": [],
      "parentBeaconBlockRoot": "0x...",
      "transactions": ["0x...", "0x..."],  // RLP-encoded transactions
      "gasLimit": "0x1c9c380"  // Optional; defaults to parent header gas limit
    }
  ]
}
```

### Txpool RPC Usage

To retrieve pending transactions from the txpool:

```bash
# Using curl
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "txpoolExt_getTxs",
    "params": [],
    "id": 1
  }'

# Response format
{
  "jsonrpc": "2.0",
  "result": [
    "0xf86d...",  // RLP-encoded transaction bytes 1
    "0xf86e...",  // RLP-encoded transaction bytes 2
    // ... more transactions up to the byte limit
  ],
  "id": 1
}
```

## Architecture

### Modular Design

Ev-reth follows a modular architecture similar to Odyssey, with clear separation of concerns:

- **`bin/ev-reth`**: The main executable binary
- **`crates/common`**: Shared utilities and constants used across all crates
- **`crates/node`**: Core node implementation including the payload builder
- **`crates/evolve`**: Evolve-specific types, RPC extensions, and integration logic
- **`crates/tests`**: Comprehensive test suite including unit and integration tests

This modular design allows for:

- Better code organization and maintainability
- Easier testing of individual components
- Clear separation between Evolve-specific and general node logic
- Reusable components for other projects

### Components

1. **EvolvePayloadBuilder** (`crates/node/src/builder.rs`)
   - Handles payload construction with transactions from Engine API
   - Manages state execution and block assembly

2. **EvolveEngineTypes** (`bin/ev-reth/src/main.rs`)
   - Custom Engine API types supporting transaction attributes
   - Payload validation and attribute processing

3. **EvolveEngineValidator** (`bin/ev-reth/src/main.rs`)
   - Modified validator for Evolve-specific requirements
   - Bypasses certain validations while maintaining security

4. **Payload Builder Missing Payload Handling** (`bin/ev-reth/src/builder.rs`)
   - Implements `on_missing_payload` to await in-progress payload builds
   - Prevents race conditions when multiple requests are made for the same payload
   - Ensures deterministic payload generation without redundant builds

5. **EvolveConsensus** (`crates/evolve/src/consensus.rs`)
   - Custom consensus implementation for Evolve
   - Allows blocks with equal timestamps (parent.timestamp <= header.timestamp)
   - Wraps standard Ethereum beacon consensus for other validations

6. **Evolve Types** (`crates/evolve/src/types.rs`)
   - Evolve-specific payload attributes and types
   - Transaction encoding/decoding utilities

7. **Evolve Txpool RPC** (`crates/evolve/src/rpc/txpool.rs`)
   - Custom RPC implementation for transaction pool queries
   - Efficient transaction retrieval with size-based limits
   - Returns RLP-encoded transaction bytes for Evolve consumption

## Fee Handling (Vaults)

ev-reth can route and credit fees to vaults configured in genesis:

- Predeploy vaults: SequencerFeeVault (coinbase), BaseFeeVault, L1FeeVault, and optional OperatorFeeVault.
- Add `ev_reth.feeHandlers` to chainspec extras (see `etc/ev-reth-genesis.json` and [docs/fee-handlers.md](docs/fee-handlers.md) for field descriptions).
- L1 fee model (V1) uses a Celestia shares-based formula: shares = ceil(compressed_size / 512) + overhead_shares; l1_fee_wei = shares × (blob_price_scalar × l1_blob_base_fee) / 10^decimals.
- Runtime: coinbase points to SequencerFeeVault; after execution, base fee, L1 fee, and optional operator fee are computed and credited to their vaults.

Note: the node loads fee handler parameters from the chainspec extras. To activate Celestia fees, supply `l1_blob_base_fee` per block (from your Celestia header feed) or wire a default in the builder; then apply credits before finalization so they reflect in the state root.

### Transaction Flow

1. Evolve submits transactions via Engine API payload attributes
2. `EvolveEnginePayloadAttributes` decodes and validates transactions
3. `EvolvePayloadBuilder` executes transactions and builds block
4. Block is returned via standard Engine API response

## Configuration

### Redirecting the Base Fee (Custom Networks Only)

On vanilla Ethereum, EIP-1559 burns the base fee. If you’re running a custom network and want that
amount to be paid to a designated address instead, `ev-reth` can credit it at block build time.

Set an environment variable before starting the node:

```bash
export EV_RETH_BASEFEE_RECIPIENT=0xYourRecipientAddressHere
./target/release/ev-reth node ...
```

What it does:
- Computes `base_fee_per_gas * gas_used` from the block header and increments the recipient’s
  balance by that amount inside the block state. This effectively “unburns” the base fee on your
  network. (Ethereum mainnet keeps burning the base fee by protocol design.)  

Implementation details:
- Uses `reth_revm::State::increment_balances` to apply the balance increment during payload build.
- No changes to chainspec or consensus are required for your custom network.

### Payload Builder Configuration

The payload builder can be configured with:

- `max_transactions`: Maximum transactions per block (default: 1000)
- `min_gas_price`: Minimum gas price requirement (default: 1 Gwei)

### Txpool RPC Configuration

The txpool RPC extension can be configured with:

- `max_txpool_bytes`: Maximum bytes of transactions to return (default: 1.85 MB)
- `max_txpool_gas`: Maximum cumulative gas for transactions to return (default: 30,000,000)

Notes:
- Both limits apply together. Selection stops when either cap is reached.
- Set a limit to `0` to disable that constraint.

CLI/env overrides:
- None for txpool gas. The RPC follows the current block gas automatically.

### Gas Limits: Block vs Txpool

- Block gas limit (per-block): Can be passed via Engine API payload attributes `gasLimit`. If omitted, ev-reth now defaults to the parent header’s gas limit (which is the genesis gas limit for the first block). The payload builder enforces this during execution and requires it to be > 0.
- Txpool gas cap (RPC selection): `txpoolExt_getTxs` follows the current block gas limit automatically. There is no CLI/env override; this keeps txpool selection aligned with execution gas by default.
- Relationship: These limits are aligned by default. Overriding the txpool cap makes them independent again; exact packing still depends on real execution.

Changing limits on a running chain:
- Per-block gas: Set `gasLimit` in Engine API payload attributes to change the block’s gas limit for that payload. Subsequent payloads will default to that new parent header gas limit unless overridden again.
- Txpool gas cap: Follows the head block’s gas limit automatically. There is no fixed-cap override; change your block gas and the RPC alignment follows.

### Node Configuration

All standard Reth configuration options are supported. Key options for Evolve integration:

- `--http`: Enable HTTP-RPC server
- `--ws`: Enable WebSocket-RPC server
- `--authrpc.port`: Engine API port (default: 8551)
- `--authrpc.jwtsecret`: Path to JWT secret for Engine API authentication

## Development

### Project Structure

```
ev-reth/
├── bin/
│   └── ev-reth/                  # Main binary
│       ├── Cargo.toml
│       └── src/
│           └── main.rs         # Binary with Engine API integration
├── crates/
│   ├── common/                 # Shared utilities and constants
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── constants.rs
│   ├── node/                   # Core node implementation
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── builder.rs     # Payload builder implementation
│   │       └── config.rs      # Configuration types
│   ├── evolve/                # Evolve-specific types
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs      # Evolve configuration
│   │       ├── consensus.rs   # Custom consensus implementation
│   │       ├── types.rs       # Evolve payload attributes
│   │       └── rpc/
│   │           ├── mod.rs
│   │           └── txpool.rs  # Txpool RPC implementation
│   └── tests/                  # Comprehensive test suite
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           └── *.rs            # Test files
├── etc/                        # Configuration files
│   └── ev-reth-genesis.json      # Genesis configuration
├── Cargo.toml                  # Workspace configuration
├── Makefile                    # Build automation
└── README.md                   # This file
```

### Running Tests

```bash
# Run all tests
make test

# Run with verbose output
make test-verbose

# Run specific test
cargo test test_name
```

### Building for Development

```bash
# Debug build
make build-dev

# Run with debug logs
make run-dev
```

## Troubleshooting

### Common Issues

1. **Transaction Decoding Errors**
   - Ensure transactions are properly RLP-encoded
   - Check that transaction format matches network requirements

2. **Block Production Failures**
   - Verify gas limits are reasonable
   - Check state availability for parent block

3. **Engine API Connection Issues**
   - Ensure JWT secret is properly configured
   - Verify Engine API port is accessible

### Debug Logging

Enable detailed logging:

```bash
RUST_LOG=debug,ev-reth=trace ./target/release/ev-reth node
```

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Make your changes with tests
4. Submit a pull request

## License

This project is dual-licensed under:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

## Acknowledgments

This project builds upon the excellent work of:

- [Reth](https://github.com/paradigmxyz/reth) - The Rust Ethereum client
- [Evolve](https://ev.xyz/) - The modular rollup framework

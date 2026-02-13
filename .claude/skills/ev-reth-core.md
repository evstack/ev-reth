---
description: This skill should be used when learning ev-reth core node architecture, understanding how payload building works, or getting started with the codebase. Use when the user asks "how does ev-reth work", "explain the architecture", "where is the payload builder", "how are transactions submitted", "what is EvolveNode", "show me the node composition", or wants to understand Engine API integration.
---

# Core Node Architecture Onboarding

## Overview

The core node logic lives in `crates/node/` and `bin/ev-reth/`. This is where ev-reth extends Reth to work with Evolve's transaction submission model.

## Key Files

### Entry Point
- `bin/ev-reth/src/main.rs` - Node binary, initializes tracing, extends RPC

### Node Composition
- `crates/node/src/node.rs` - `EvolveNode` unit struct with trait implementations
- `crates/node/src/lib.rs` - Public API exports

### Payload Building
- `crates/node/src/builder.rs` - `EvolvePayloadBuilder` - executes transactions, builds blocks
- `crates/node/src/payload_service.rs` - Integrates builder with Reth's payload service
- `crates/node/src/attributes.rs` - `EvolveEnginePayloadBuilderAttributes`

### Validation
- `crates/node/src/validator.rs` - `EvolveEngineValidator` - custom block validation

### Configuration
- `crates/node/src/config.rs` - `EvolvePayloadBuilderConfig`, parses chainspec extras
- `crates/node/src/chainspec.rs` - `EvolveChainSpecParser` with EIP-1559 config parsing
- `crates/node/src/args.rs` - CLI argument handling
- `crates/node/src/error.rs` - Error types

### Execution
- `crates/node/src/executor.rs` - EVM config and executor wiring

## Architecture

### Transaction Flow (Key Innovation)

Unlike standard Ethereum, ev-reth accepts transactions directly through Engine API:

```
engine_forkchoiceUpdatedV3 (with transactions in payload attributes)
    ↓
EvolveEnginePayloadBuilderAttributes (decodes transactions)
    ↓
EvolvePayloadBuilder.build_payload()
    ↓
Execute transactions against current state
    ↓
Sealed block returned via engine_getPayloadV3
```

### Node Composition Pattern

`EvolveNode` is a unit struct that implements `NodeTypes` and `Node<N>` traits:

```rust
pub struct EvolveNode;

impl NodeTypes for EvolveNode {
    type Primitives = EthPrimitives;
    type ChainSpec = ChainSpec;
    type StateCommitment = MerklePatriciaTrie;
    type Storage = EthStorage;
    type Payload = EthEngineTypes;
}
```

The composition happens via trait implementations, connecting:
- `EvolveEngineTypes` for custom payload types
- `EvolveEngineValidator` for relaxed validation
- `EvolvePayloadBuilderBuilder` for custom block building
- `EvolveConsensusBuilder` from `evolve_ev_reth::consensus`

### Validator Customizations

`EvolveEngineValidator` bypasses certain checks for Evolve compatibility:
- Block hash validation bypassed (Evolve uses prev block's apphash)
- Equal timestamp blocks allowed
- Custom gas limits per payload supported

### Chainspec Extensions

The chainspec parser supports Evolve-specific extras via `EvolveEip1559Config`:
- EIP-1559 custom parameters (base fee settings)
- Additional fields parsed from `evolve` key in chainspec extras

## Key Design Decisions

1. **No Mempool** - Transactions submitted directly via Engine API
2. **Relaxed Validation** - Block hashes not validated (Evolve-specific)
3. **Configurable Gas Limits** - Per-payload gas limits supported
4. **Modular Builder** - Separates concerns between general and Evolve-specific logic

## Development Commands

```bash
make build      # Release build
make run-dev    # Run with debug logs
make test-node  # Test node crate
```

## Exploration Starting Points

1. Start with `bin/ev-reth/src/main.rs` for entry point
2. Read `crates/node/src/node.rs` for component composition
3. Read `crates/node/src/builder.rs` for payload building (this is the heart)
4. Check `crates/node/src/validator.rs` for validation customizations
5. See `crates/node/src/chainspec.rs` for config parsing

<!-- Last reviewed: 2026-02-13 -->

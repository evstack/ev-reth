# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common Development Commands

### Building
- **Release build**: `make build`
- **Debug build**: `make build-dev`
- **Build all workspace members**: `make build-all`

### Testing
- **Run all tests**: `make test`
- **Run tests with output**: `make test-verbose`
- **Unit tests only**: `make test-unit`
- **Integration tests**: `make test-integration`
- **Test specific crate**: `make test-node`, `make test-rollkit`, `make test-common`

### Code Quality
- **Format code**: `make fmt`
- **Check formatting**: `make fmt-check`
- **Run linter**: `make lint`
- **Run all checks**: `make check-all`

### Running the Node
- **Run with defaults**: `make run`
- **Run with debug logs**: `make run-dev`
- **Direct execution**: `./target/release/lumen node --chain <CHAIN_SPEC> --datadir <DATA_DIR> --http --ws`

## High-Level Architecture

Lumen is a specialized Ethereum execution client built on Reth that integrates with Rollkit. The key architectural innovation is accepting transactions directly through the Engine API instead of the traditional mempool.

### Core Components

1. **RollkitPayloadBuilder** (`crates/node/src/builder.rs`)
   - Accepts transactions from Engine API payload attributes
   - Executes transactions and builds blocks
   - Manages state transitions

2. **RollkitEngineTypes** (`bin/lumen/src/main.rs`)
   - Custom Engine API types supporting transaction submission
   - Handles payload attribute validation and processing

3. **RollkitEngineValidator** (`bin/lumen/src/main.rs`)
   - Modified validator that bypasses certain checks for Rollkit compatibility
   - Maintains security while allowing flexible block production

### Transaction Flow
1. Rollkit submits transactions via `engine_forkchoiceUpdatedV3` with transactions in payload attributes
2. Transactions are decoded from RLP format and validated
3. Payload builder executes transactions against current state
4. Block is constructed and returned via Engine API

### Key Design Decisions
- Transactions bypass the mempool entirely, submitted directly via Engine API
- Block validation is relaxed for Rollkit-produced blocks (hash validation bypassed)
- Custom gas limits can be specified per payload
- Modular workspace structure separates concerns between general node logic and Rollkit-specific features

### Testing Strategy
- Unit tests for individual components
- Integration tests in `crates/tests/` covering:
  - Engine API interactions
  - Payload building with transactions
  - State execution validation
  - Rollkit-specific scenarios
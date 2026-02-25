# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common Development Commands

### Building
- **Release build**: `just build`
- **Debug build**: `just build-dev`
- **Build all workspace members**: `just build-all`

### Testing
- **Run all tests**: `just test`
- **Run tests with output**: `just test-verbose`
- **Unit tests only**: `just test-unit`
- **Integration tests**: `just test-integration`
- **Test specific crate**: `just test-node`, `just test-evolve`, `just test-common`

### Code Quality
- **Format code**: `just fmt`
- **Check formatting**: `just fmt-check`
- **Run linter**: `just lint`
- **Run all checks**: `just check-all`

### Running the Node
- **Run with defaults**: `just run`
- **Run with debug logs**: `just run-dev`
- **Direct execution**: `./target/release/lumen node --chain <CHAIN_SPEC> --datadir <DATA_DIR> --http --ws`

## High-Level Architecture

Ev-reth is a specialized Ethereum execution client built on Reth that integrates with Evolve. The key architectural innovation is accepting transactions directly through the Engine API instead of the traditional mempool.

### Core Components

1. **EvolvePayloadBuilder** (`crates/node/src/builder.rs`)
   - Accepts transactions from Engine API payload attributes
   - Executes transactions and builds blocks
   - Manages state transitions

2. **EvolveEngineTypes** (`bin/ev-reth/src/main.rs`)
   - Custom Engine API types supporting transaction submission
   - Handles payload attribute validation and processing

3. **EvolveEngineValidator** (`bin/ev-reth/src/main.rs`)
   - Modified validator that bypasses certain checks for Evolve compatibility
   - Maintains security while allowing flexible block production

### Transaction Flow
1. Ev-reth submits transactions via `engine_forkchoiceUpdatedV3` with transactions in payload attributes
2. Transactions are decoded from RLP format and validated
3. Payload builder executes transactions against current state
4. Block is constructed and returned via Engine API

### Key Design Decisions
- Transactions bypass the mempool entirely, submitted directly via Engine API
- Block validation is relaxed for ev-node produced blocks (hash validation bypassed)
- Custom gas limits can be specified per payload
- Modular workspace structure separates concerns between general node logic and Evolve-specific features

### Testing Strategy
- Unit tests for individual components
- Integration tests in `crates/tests/` covering:
  - Engine API interactions
  - Payload building with transactions
  - State execution validation
  - Evolve-specific scenarios

---
description: This skill should be used when the user asks about "ev-reth contracts", "FeeVault", "AdminProxy", "Permit2", "fee distribution", "Foundry deployment scripts", "genesis allocations", or wants to understand how base fees are redirected and distributed.
---

# Contracts Onboarding

## Overview

The contracts live in `contracts/` and use Foundry for development. There are two main contracts:

1. **AdminProxy** (`src/AdminProxy.sol`) - Bootstrap contract for admin addresses at genesis
2. **FeeVault** (`src/FeeVault.sol`) - Collects base fees and distributes them between configured recipients
3. **Permit2** (`lib/permit2`) - Uniswap's canonical token approval manager, deployed at genesis via `ev-deployer` (no Foundry deploy script — bytecode is embedded in Rust)

## Key Files

### Contract Sources
- `contracts/src/AdminProxy.sol` - Transparent proxy pattern for admin control
- `contracts/src/FeeVault.sol` - Fee collection and distribution logic
- `contracts/lib/permit2` - Uniswap Permit2 submodule (bytecode used by ev-deployer)

### Deployment Scripts
- `contracts/script/DeployFeeVault.s.sol` - FeeVault deployment with CREATE2
- `contracts/script/GenerateAdminProxyAlloc.s.sol` - Admin proxy allocation for genesis
- `contracts/script/GenerateFeeVaultAlloc.s.sol` - Fee vault allocation for genesis

### Tests
- `contracts/test/AdminProxy.t.sol` - AdminProxy test suite
- `contracts/test/FeeVault.t.sol` - FeeVault test suite

## Architecture

### AdminProxy
The AdminProxy contract provides a bootstrap mechanism for setting admin addresses at genesis. It uses a transparent proxy pattern allowing upgrades.

### FeeVault
The FeeVault serves as the destination for redirected base fees (instead of burning them). Key responsibilities:
- Receive base fees from block production
- Distribute accumulated fees between configured recipients
- Manage withdrawal permissions

### Permit2
Uniswap's canonical token approval manager deployed at genesis. Unlike AdminProxy and FeeVault, Permit2 has no Foundry deploy script — its bytecode is embedded directly in the Rust `ev-deployer` (`bin/ev-deployer/src/contracts/permit2.rs`), which patches EIP-712 immutables (chain ID, domain separator) at genesis time.

## Connection to Rust Code

The contracts integrate with ev-reth through:
1. **Base Fee Redirect** - `crates/ev-revm/src/base_fee.rs` redirects fees to the configured sink address
2. **Chainspec Config** - `crates/node/src/config.rs` defines `base_fee_sink` field for the fee recipient address
3. **Genesis Allocation** - Scripts generate allocations included in chainspec

## Development Commands

```bash
cd contracts

# Build contracts
forge build

# Run tests
forge test

# Run specific test
forge test --match-test testFeeCollection

# Generate allocations
forge script script/GenerateFeeVaultAlloc.s.sol
```

## Exploration Starting Points

1. Read `contracts/src/FeeVault.sol` for fee handling logic
2. Read `contracts/src/AdminProxy.sol` for admin patterns
3. Check `contracts/script/` for deployment patterns
4. See how `crates/ev-revm/src/base_fee.rs` interacts with the sink address

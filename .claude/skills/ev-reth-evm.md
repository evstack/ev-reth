---
description: This skill should be used when the user asks about "EVM customizations", "base fee redirect", "fee sink", "mint precompile", "native token minting", "contract size limit", "EvEvmFactory", "EvHandler", or wants to understand how ev-reth modifies EVM execution behavior.
---

# EVM Customizations Onboarding

## Overview

EVM customizations live in `crates/ev-revm/` and `crates/ev-precompiles/`. These modify how the EVM executes transactions.

## Key Files

### EVM Factory
- `crates/ev-revm/src/factory.rs` - `EvEvmFactory` wraps `EthEvmFactory`
- `crates/ev-revm/src/evm.rs` - `EvEvm`, `DefaultEvEvm` implementations
- `crates/ev-revm/src/config.rs` - EVM configuration structs

### Handlers
- `crates/ev-revm/src/handler.rs` - `EvHandler` for execution handling
- `crates/ev-revm/src/base_fee.rs` - `BaseFeeRedirect` logic

### Precompiles
- `crates/ev-precompiles/src/mint.rs` - `MintPrecompile` at address 0xF100

### Shared
- `crates/common/src/constants.rs` - Shared constants

## Architecture

### EvEvmFactory

Wraps Reth's `EthEvmFactory` to inject custom behavior. See `crates/ev-revm/src/factory.rs:103-108`:

```rust
pub struct EvEvmFactory<F> {
    inner: F,
    redirect: Option<BaseFeeRedirectSettings>,
    mint_precompile: Option<MintPrecompileSettings>,
    contract_size_limit: Option<ContractSizeLimitSettings>,
}
```

### Base Fee Redirect

Instead of burning base fees, redirects them to a configurable address. See `crates/ev-revm/src/factory.rs:27-49`:

```rust
pub struct BaseFeeRedirectSettings {
    redirect: BaseFeeRedirect,      // Contains fee_sink address
    activation_height: u64,         // When redirect activates
}
```

The `EvHandler` overrides `reward_beneficiary` (in `handler.rs:126-141`) to credit the sink address with base fees before paying the standard tip to the block producer.

### Mint Precompile (0xF100)

Custom precompile for native token minting/burning at address `0xF100`. See `crates/ev-precompiles/src/mint.rs`.

**INativeToken Interface** (5 functions):
```solidity
interface INativeToken {
    function mint(address to, uint256 amount) external;
    function burn(address from, uint256 amount) external;
    function addToAllowList(address account) external;
    function removeFromAllowList(address account) external;
    function allowlist(address account) external view returns (bool);
}
```

Settings in `crates/ev-revm/src/factory.rs:52-74`:
```rust
pub struct MintPrecompileSettings {
    admin: Address,             // Who can mint/burn and manage allowlist
    activation_height: u64,     // When precompile activates
}
```

### Contract Size Limits

Override EIP-170 default (24KB) contract size limit. See `crates/ev-revm/src/factory.rs:77-99`:

```rust
pub struct ContractSizeLimitSettings {
    limit: usize,               // Custom limit in bytes
    activation_height: u64,     // When limit changes
}
```

## Configuration Flow

1. Chainspec defines settings in `extras` field
2. `EvolveChainSpecParser` parses into config structs
3. `EvEvmFactory` receives settings at construction
4. Settings applied during EVM execution based on block height

## Key Design Decisions

1. **Configurable Activation** - All features have activation heights for upgrades
2. **Wrapper Pattern** - `EvEvmFactory` wraps standard factory, minimizing changes
3. **Admin Control** - Mint precompile requires admin authorization (or allowlist)
4. **Fee Preservation** - Base fees collected rather than burned (for bridging)

## Development Commands

```bash
cargo test -p ev-revm        # Test EVM crate
cargo test -p ev-precompiles # Test precompiles
```

## Exploration Starting Points

1. Start with `crates/ev-revm/src/factory.rs` for the wrapper pattern
2. Read `crates/ev-revm/src/handler.rs:126-141` for `reward_beneficiary` override
3. Read `crates/ev-precompiles/src/mint.rs` for precompile implementation
4. Check `crates/ev-revm/src/base_fee.rs` for redirect logic
5. See `crates/node/src/config.rs` for how settings are configured

# Upgrade Guide: v0.4.0

This guide covers the changes in ev-reth v0.4.0 since v0.3.0.

## Breaking Changes

### Reth Upgraded to v2.0.0

The underlying Reth dependency has been upgraded from v1.11.x to v2.0.0. This is a major version bump that affects payload building, EVM execution, and transaction primitives.

**Key upstream changes:**

- **revm 36.0.0:** New EVM internals. The `is_pure` method was removed from the `Precompile` trait, and `TransactionEnv` was renamed to `TransactionEnvMut`.
- **alloy-evm 0.30.0:** Aligned with reth v2.0.0. `TryIntoTxEnv` now takes 3 generic parameters (was 2).
- **reth-primitives removed:** The monolithic `reth-primitives` crate was deleted upstream. Imports migrated to `alloy_consensus` and `reth_ethereum_primitives`.
- **Payload building rework:** `PayloadBuilderAttributes` trait merged into `PayloadAttributes`. `PayloadConfig` now requires a `payload_id` field. `BuildArguments` now requires `execution_cache` and `trie_handle` fields.
- **BlockEnv changes:** `BlockEnv` now requires a `slot_num` field. `set_state_clear_flag` was removed (handled by EVM spec).
- **BlockBuilder::finish:** Now accepts a precomputed state root parameter.

**Action Required:** Rebuild from source. If you have custom code that imports from `reth-primitives`, update imports to `alloy_consensus` or `reth_ethereum_primitives`.

### Txpool Fallback Restricted to Dev Mode

The payload builder's txpool fallback (pulling pending transactions when Engine API attributes are empty) is now **only enabled in `--dev` mode**.

Previously, this fallback was always active, which could cause non-deterministic block contents in production when Engine API attributes were empty. This is now gated behind the `--dev` flag.

**Action Required:** If your setup relied on txpool fallback in production mode, you must switch to providing transactions via Engine API attributes.

### Build System: Makefile to Justfile

The project build system has migrated from `Makefile` to [just](https://github.com/casey/just). If you had scripts or CI referencing `make` commands, update them to use `just`.

## New Features

### ev-deployer CLI

A new CLI tool for generating genesis alloc entries. It embeds contract runtime bytecodes directly in the binary, eliminating the need for external contract compilation.

**Commands:**

| Command | Description |
|---------|-------------|
| `ev-deployer init` | Generate a starter TOML config with all supported contracts |
| `ev-deployer genesis` | Build genesis alloc JSON from config |
| `ev-deployer compute-address` | Look up a configured contract's deterministic address |

**Workflow:**

```bash
# Generate config template
ev-deployer init --output deploy.toml

# Edit deploy.toml with your contracts and settings

# Generate alloc JSON
ev-deployer genesis --config deploy.toml --output alloc.json

# Or merge into existing genesis
ev-deployer genesis --config deploy.toml --merge-into genesis.json --output genesis-out.json
```

### ev-dev Local Development Chain

One-command local development chain for Evolve, similar to Hardhat Node or Anvil.

**Key features:**

- Pre-funded Hardhat accounts (10 by default, up to 20) with 1,000,000 ETH each
- Chain ID 1234, 30M gas limit, Cancun hardfork
- All Evolve features enabled: base fee redirect, 128KB contract limit, mint precompile, type 0x76 transactions
- Transient state (resets on restart)
- Compatible with Foundry, Hardhat, ethers.js, viem

**Usage:**

```bash
ev-dev --port 8545 --block-time 1 --accounts 10
```

Set `--block-time 0` for on-demand block production (mine on transaction).

### Transaction Sponsor Service

A Fastify-based JSON-RPC proxy that signs EvNode (0x76) transactions as sponsor on behalf of users.

**How it works:**

1. Client sends unsigned 0x76 transaction via `eth_sendRawTransaction`
2. Service intercepts, validates against policy, signs as sponsor, forwards to node
3. All other RPC calls are transparently proxied
4. Zero client code changes -- just point your RPC URL to the service

**Configuration (environment variables):**

| Variable | Description | Default |
|----------|-------------|---------|
| `RPC_URL` | Upstream ev-reth node URL | -- |
| `CHAIN_ID` | Chain ID for validation | -- |
| `SPONSOR_PRIVATE_KEY` | Sponsor wallet key | -- |
| `MAX_GAS_LIMIT_PER_TX` | Max gas per sponsored tx | 500,000 |
| `MAX_FEE_PER_GAS_LIMIT` | Max fee per gas ceiling | 100 Gwei |
| `MIN_SPONSOR_BALANCE` | Min sponsor balance threshold | 1 ETH |
| `PORT` | Service listen port | 3000 |

### Granular Tracing and Observability

Instrumentation spans added throughout critical code paths for detailed observability.

**Covered operations:**

- `build_payload` -- parent_hash, tx_count, gas_limit, duration_ms
- `try_build` -- payload_id, duration_ms
- `ensure_well_formed_payload` -- block_number, block_hash, duration_ms
- `validate_transaction` -- origin, tx_hash, duration_ms
- `execute_tx` -- debug-level spans with duration_ms
- `build_empty_payload`, `parse_evolve_payload`, `validate_evnode`

**Independent trace level control:**

New environment variable `EV_TRACE_LEVEL` controls OTLP span export independently from `RUST_LOG`. This lets operators run with clean stdout logs while exporting debug-level spans to Jaeger or other backends:

```bash
RUST_LOG=info EV_TRACE_LEVEL=debug ev-reth node ...
```

## Bug Fixes

### EIP-2718 Payload Decode Fix

Fixed a bug where the payload builder used `network_decode` (devp2p RLP wrapping) instead of `decode_2718_exact` (Engine API spec: opaque EIP-2718 bytes). This could silently drop valid type 0x76 EvNode transactions and EIP-1559/EIP-2930 transactions whose bytes were valid EIP-2718 but lacked wire-format RLP wrapping.

### Deploy Allowlist Test Coverage

Additional test coverage for deploy allowlist edge cases, ensuring consistent enforcement across all transaction types and gas specification scenarios.

## Upgrade for Existing Networks

v0.4.0 is a drop-in replacement for v0.3.0. No chainspec modifications are required.

1. All features from v0.3.0 (EvNode 0x76 transactions, Osaka/EOF, viem client) continue to work unchanged
2. The EIP-2718 decode fix takes effect immediately
3. Txpool fallback is now dev-mode only -- production deployments using Engine API are unaffected
4. New tools (`ev-deployer`, `ev-dev`, sponsor service) are opt-in

## Migration Checklist

- [ ] Rebuild from source with reth v2.0.0 dependencies
- [ ] If using custom build scripts: migrate `make` commands to `just`
- [ ] If using custom code that imports `reth-primitives`: update imports
- [ ] If relying on txpool fallback in production: switch to Engine API attributes
- [ ] Review `EV_TRACE_LEVEL` for your observability setup
- [ ] Test the upgrade on a local/testnet deployment using `ev-dev`
- [ ] Coordinate upgrade timing with network validators/operators
- [ ] Deploy new ev-reth binary
- [ ] Verify node starts and syncs correctly
- [ ] Verify existing transactions and block production continue working

## Related Documentation

- [Upgrade Guide: v0.3.0](UPGRADE-v0.3.0.md) -- previous version changes
- [ADR 003: Typed Transactions for Sponsorship and Batch Calls](adr/ADR-0003-typed-transactions-sponsorship.md)
- [Permissioned EVM Guide](guide/permissioned-evm.md)
- [Fee System Guide](guide/fee-systems.md)

## Questions?

For issues or questions about the upgrade, please open an issue at <https://github.com/evstack/ev-reth/issues>

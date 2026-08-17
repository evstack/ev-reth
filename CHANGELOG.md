# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0] - 2026-08-17

### Added

- Optional proposer-control precompile at `0xF101` for execution-owned ev-node proposer rotation. Enable with `proposerControlAdmin` in `config.evolve`; optional `proposerControlActivationHeight` (defaults to `0`) and `initialNextProposer` (32-byte word). Exposes `evolve_getNextProposer` only when the admin is configured ([#228](https://github.com/evstack/ev-reth/pull/228))

### Changed

- Upgraded Reth from v2.2.0 to v2.5.0, with matching Alloy 2.3, revm 42.0.1, alloy-evm 0.38.0, reth-codecs / reth-primitives-traits 0.6, and related dependency updates. Custom EVM batching, RPC, codec, and txpool integrations were migrated to the v2.5 APIs, including batch runtime-gas accounting ([#266](https://github.com/evstack/ev-reth/pull/266), [#338](https://github.com/evstack/ev-reth/pull/338), [#363](https://github.com/evstack/ev-reth/pull/363))

### Removed

- `ev-deployer` CLI and `ev-dev` local-development TUI. Genesis allocs and local nets should use the Foundry scripts under `contracts/` and a standard ev-reth / ev-node stack ([#349](https://github.com/evstack/ev-reth/pull/349))

## [0.4.1] - 2026-06-16

### Added

- Lightweight custom chainspec mode (`--chain light:<path-or-json>`) for warm datadir restarts without reparsing large genesis allocs, including automatic `.light.json` sidecar generation from full file-based genesis specs ([#253](https://github.com/evstack/ev-reth/pull/253))

### Changed

- Upgraded Reth from v2.0.0 to v2.2.0, with matching Alloy 2.x, revm 38.0.0, alloy-evm 0.34.0, reth-codecs 0.3.1, Tokio 1.52, Clap 4.6, and related dependency updates ([#223](https://github.com/evstack/ev-reth/pull/223), [#236](https://github.com/evstack/ev-reth/pull/236), [#250](https://github.com/evstack/ev-reth/pull/250))
- Updated Engine API payload handling for Amsterdam-era fields, including payload attribute slot numbers and V6 execution payload block access list / slot number propagation.
- Updated EvNode batch transaction gas validation and block execution accounting for Amsterdam/EIP-8037 state gas and transaction gas-limit cap semantics.
- Changed FeeVault from Hyperlane `HypNativeMinter` bridging to a simpler native-token splitter: `distribute()` now sends the bridge share to `bridgeRecipient` and the remainder to `otherRecipient`; storage layout, constructor args, events, scripts, and docs were updated accordingly.
- Split `ev-deployer init` into explicit mode subcommands; use `ev-deployer init genesis` or `ev-deployer init deploy` instead of the old single `init` command.
- `ev-deployer` config validation now rejects duplicate deployment addresses and requires addresses only for genesis mode, while deploy mode computes addresses from CREATE2 inputs.

### Fixed

- Tracing fields now use display formatting where appropriate, improving structured logs for addresses, hashes, gas limits, and errors ([#255](https://github.com/evstack/ev-reth/pull/255))
- Native minting precompile authorization, ABI decode, balance overflow, and insufficient-balance failures now halt the precompile call instead of surfacing as fatal precompile errors
- Preallocated block receipt storage from the transaction-count hint to reduce avoidable allocations during block execution
- Preserved existing lightweight chainspec sidecars when a new full genesis produces a different genesis hash, writing a hash-specific sidecar instead of overwriting the old one ([#253](https://github.com/evstack/ev-reth/pull/253))

## [0.4.0] - 2026-04-17

> **Note:** v0.3.0 was never released (only `v0.3.0-beta` shipped). Changes originally staged under v0.3.0 are rolled into v0.4.0 below so the upgrade path is v0.2.x → v0.4.0.

### Added

- `ev-deployer` CLI (`bin/ev-deployer`) for generating genesis alloc entries with embedded contract bytecodes ([#167](https://github.com/evstack/ev-reth/pull/167))
- `ev-dev` binary (`bin/ev-dev`): one-command local development chain with pre-funded Hardhat accounts, similar to Anvil or Hardhat Node
- Transaction sponsor service (`bin/sponsor-service`) for signing EvNode transactions on behalf of users via JSON-RPC proxy ([#141](https://github.com/evstack/ev-reth/pull/141))
- Granular tracing instrumentation spans across payload building, transaction validation, and EVM execution
- `EV_TRACE_LEVEL` env var to control OTLP span export verbosity independently from `RUST_LOG` stdout log level ([#156](https://github.com/evstack/ev-reth/issues/156))
- EvNode transaction type (0x76) with atomic batch calls and fee-payer sponsorship ([#103](https://github.com/evstack/ev-reth/pull/103))
- Viem client library (`@evstack/evnode-viem`) for building, signing, and sponsoring EvNode transactions ([#112](https://github.com/evstack/ev-reth/pull/112))
- End-to-end tests for the EvNode client ([#118](https://github.com/evstack/ev-reth/pull/118))
- Tini init process in Docker images for proper signal handling ([#115](https://github.com/evstack/ev-reth/pull/115))

### Changed

- Upgraded Reth from v1.8.4 to v2.0.0 with Osaka/EOF hardfork support, Storage V2, revm 36.0.0, and alloy-evm 0.30.0 ([#106](https://github.com/evstack/ev-reth/pull/106), [#207](https://github.com/evstack/ev-reth/pull/207))
- `reth-primitives` imports migrated to `alloy_consensus` and `reth_ethereum_primitives` (upstream crate removed)
- Txpool fallback (pulling pending transactions when Engine API attributes are empty) restricted to `--dev` mode only
- Migrated build system from Makefile to Justfile
- Disabled default features on several reth crates to unblock SP1 proving work ([#111](https://github.com/evstack/ev-reth/pull/111))
- Removed unused `thiserror` dependency from `ev-precompiles` crate

### Fixed

- Payload builder now uses `decode_2718_exact` instead of `network_decode` for Engine API payloads, fixing silent drops of valid type 0x76 and EIP-1559/EIP-2930 transactions ([#219](https://github.com/evstack/ev-reth/pull/219))
- Payload builder now pulls pending transactions from the txpool in `--dev` mode, fixing `cast send` and other RPC-submitted transactions not being included in blocks
- Txpool now uses sponsor balance for pending/queued ordering in sponsored EvNode transactions, and validates executor balance separately for call value transfers ([#141](https://github.com/evstack/ev-reth/pull/141))
- Permissioned EVM deploy allowlist validation when gas is explicitly specified ([#122](https://github.com/evstack/ev-reth/pull/122))
- Additional test coverage for deploy allowlist edge cases across all transaction types

## [0.2.2] - 2026-01-22

### Added

- Permissioned EVM support allowing configurable address-based access control ([#100](https://github.com/evstack/ev-reth/pull/100))
- EIP-1559 settings to chain configuration for customizing base fee parameters ([#99](https://github.com/evstack/ev-reth/pull/99))
- AdminProxy contract for administrative operations ([#97](https://github.com/evstack/ev-reth/pull/97))
- ADR 003: typed sponsorship transactions and batch execution documentation ([#96](https://github.com/evstack/ev-reth/pull/96))
- Fee system guide documentation ([#101](https://github.com/evstack/ev-reth/pull/101))

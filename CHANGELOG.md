# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `ev-deployer` CLI (`bin/ev-deployer`) for generating genesis alloc entries with embedded contract bytecodes ([#167](https://github.com/evstack/ev-reth/pull/167))
- `ev-dev` binary (`bin/ev-dev`): one-command local development chain with pre-funded Hardhat accounts, similar to Anvil or Hardhat Node
- Transaction sponsor service (`bin/sponsor-service`) for signing EvNode transactions on behalf of users via JSON-RPC proxy ([#141](https://github.com/evstack/ev-reth/pull/141))
- Granular tracing instrumentation spans across payload building, transaction validation, and EVM execution
- `EV_TRACE_LEVEL` env var to control OTLP span export verbosity independently from `RUST_LOG` stdout log level ([#156](https://github.com/evstack/ev-reth/issues/156))

### Changed

- Upgraded Reth from v1.11.x to v2.0.0 with Storage V2 support, revm 36.0.0, and alloy-evm 0.30.0 ([#207](https://github.com/evstack/ev-reth/pull/207))
- `reth-primitives` imports migrated to `alloy_consensus` and `reth_ethereum_primitives` (upstream crate removed)
- Txpool fallback (pulling pending transactions when Engine API attributes are empty) restricted to `--dev` mode only
- Migrated build system from Makefile to Justfile
- Removed unused `thiserror` dependency from `ev-precompiles` crate

### Fixed

- Payload builder now uses `decode_2718_exact` instead of `network_decode` for Engine API payloads, fixing silent drops of valid type 0x76 and EIP-1559/EIP-2930 transactions ([#219](https://github.com/evstack/ev-reth/pull/219))
- Payload builder now pulls pending transactions from the txpool in `--dev` mode, fixing `cast send` and other RPC-submitted transactions not being included in blocks
- Txpool now uses sponsor balance for pending/queued ordering in sponsored EvNode transactions, and validates executor balance separately for call value transfers ([#141](https://github.com/evstack/ev-reth/pull/141))
- Additional test coverage for deploy allowlist edge cases across all transaction types

## [0.3.0] - 2026-02-23

### Added

- EvNode transaction type (0x76) with atomic batch calls and fee-payer sponsorship ([#103](https://github.com/evstack/ev-reth/pull/103))
- Viem client library (`@evstack/evnode-viem`) for building, signing, and sponsoring EvNode transactions ([#112](https://github.com/evstack/ev-reth/pull/112))
- End-to-end tests for the EvNode client ([#118](https://github.com/evstack/ev-reth/pull/118))
- Tini init process in Docker images for proper signal handling ([#115](https://github.com/evstack/ev-reth/pull/115))

### Fixed

- Permissioned EVM deploy allowlist validation when gas is explicitly specified ([#122](https://github.com/evstack/ev-reth/pull/122))

### Changed

- Upgraded Reth from v1.8.4 to v1.11.0 with Osaka hardfork and EOF support ([#106](https://github.com/evstack/ev-reth/pull/106))
- Disabled default features on several reth crates to unblock SP1 proving work ([#111](https://github.com/evstack/ev-reth/pull/111))

## [0.2.2] - 2026-01-22

### Added

- Permissioned EVM support allowing configurable address-based access control ([#100](https://github.com/evstack/ev-reth/pull/100))
- EIP-1559 settings to chain configuration for customizing base fee parameters ([#99](https://github.com/evstack/ev-reth/pull/99))
- AdminProxy contract for administrative operations ([#97](https://github.com/evstack/ev-reth/pull/97))
- ADR 003: typed sponsorship transactions and batch execution documentation ([#96](https://github.com/evstack/ev-reth/pull/96))
- Fee system guide documentation ([#101](https://github.com/evstack/ev-reth/pull/101))

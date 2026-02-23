# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Add missing payload attribute extraction in `EvolvePayloadBuilder` to properly handle transactions submitted via Engine API ([#33](https://github.com/evstack/ev-reth/pull/33))
- Remove unused configuration parameters to clean up codebase ([#32](https://github.com/evstack/ev-reth/pull/32))
- Ensure `stateRoot` follows Ethereum post-state semantics to avoid false fork reports caused by height-1 root mismatches

### Changed

- Use `best_transactions` instead of `pending_transactions` queue for improved transaction selection logic ([#29](https://github.com/evstack/ev-reth/pull/29))
